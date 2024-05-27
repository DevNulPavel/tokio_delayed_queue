use crate::{item::DelayItem, reserve::ReserveWaker, sleep::SleepLocal};
use async_condvar_fair::{Baton, BatonExt, Condvar};
use parking_lot::Mutex;
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{atomic::Ordering, Arc},
    task::Poll,
    time::Instant,
};

////////////////////////////////////////////////////////////////////////////////

type CondvarWaitFuture<'a> = dyn Future<Output = Option<Baton<'a>>> + Send + 'a;

////////////////////////////////////////////////////////////////////////////////

/// Delayed queue future.
// Футура проверки доступности нового итема
pub struct DelayedPopFuture<'a, T> {
    // /// Нотифаер для отправки уведомлений
    pub(super) size_condvar: &'a Condvar,

    // /// Нотифаер для отправки уведомлений
    pub(super) reserve_condvar: &'a Condvar,

    // /// Сама очередь непосредственно
    pub(super) queue: &'a Mutex<VecDeque<DelayItem<T>>>,

    // /// Футура возможного ожидания новых итемов
    pub(super) condvar_future: Option<Pin<Box<CondvarWaitFuture<'a>>>>,

    // /// Футура ожидания нового итема
    // TODO: Здесь можно было бы не аллоцировать, а как есть футуру использовать,
    // но тогда пришлось бы использовать pin_project или unsafe
    // для того, чтобы обеспечить гарантию того, что футура у нас приклеена к стеку.
    pub(super) sleep_future: Option<SleepLocal>,

    // /// Отслеживание отмены ожидания футуры
    pub(super) reserve_waker: Option<ReserveWaker<'a>>,

    // /// Какой это у нас идентификатор футуры, который зарезервировал итем
    pub(super) future_id: u64,
}

// Помечаем явно, что у нас эта самая футура не привязана никак к расположению своему.
// TODO: Но вроде бы это и так у нас будет автоматически?
impl<'a, T> Unpin for DelayedPopFuture<'a, T> {}

// Явно помечаем дополнительно, что у нас IoHandle не является запинированным если S: Source.
// Видимо, это нужно чтобы указать Unpin только для определенных типов S.
// impl<'a, T> Unpin for DelayedPop<'a, T> where T: Unpin {}

impl<'a, T: Send> Future for DelayedPopFuture<'a, T> {
    type Output = T;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        'main_loop: loop {
            // Уже была создана футура для ожидания ранее?
            if let Some(condvar_future) = self.as_mut().condvar_future.as_mut() {
                // Полим один раз для проверки, регистрируется пробуждение
                match condvar_future.as_mut().poll(cx) {
                    // Еще не готово
                    Poll::Pending => {
                        // Ждем очередного пробуждения
                        return Poll::Pending;
                    }
                    // Что-то оказалось готово, продолжаем
                    Poll::Ready(res) => {
                        // Если ожидание сработало, то обрабатываем событие
                        res.dispose();

                        // Уничтожаем футуру - она отработала
                        self.as_mut().condvar_future.take();

                        // Идем на новую итерацию проверки
                        continue 'main_loop;
                    }
                }
            }

            // Уже была создана футура для ожидания ранее?
            if let Some(sleep_future) = self.as_mut().sleep_future.as_mut() {
                // Полим один раз для проверки, регистрируется пробуждение
                match Pin::new(sleep_future).poll(cx) {
                    // Еще не готово
                    Poll::Pending => {
                        // Ждем очередного пробуждения
                        return Poll::Pending;
                    }
                    // Что-то оказалось готово, продолжаем
                    Poll::Ready(_) => {
                        // Уничтожаем футуру - она отработала
                        self.as_mut().sleep_future.take();

                        // Идем на новую итерацию проверки
                        continue 'main_loop;
                    }
                }
            }

            // Берем блокировку короткую для очереди
            let mut lock = self.queue.lock();

            // Смотрим наличие итема
            if let Some(front_val) = lock.front_mut() {
                // Идентификатор футуры, которая зарезервировала этот итем
                let reserved_id = front_val.reserved.load(Ordering::Acquire);

                // Данный итем кем-то другим зарезервирован уже, но не нами же?
                if (reserved_id > 0) && (reserved_id != self.future_id) {
                    // Создаем футуру ожидания
                    let wait_future = self.reserve_condvar.wait_no_relock(lock);

                    // Раз блокировка каждый раз новая, то и футура для
                    // просыпания тоже пусть будет новая каждый раз
                    let prev = self.as_mut().condvar_future.replace(Box::pin(wait_future));

                    // Прошлой футуры быть не должно здесь
                    assert!(prev.is_none(), "Previous condvar wait should not exist");

                    // Полить будем на следующей итерации
                    continue 'main_loop;
                }
                // Футуры еще не было создано для ожидания,
                // но время еще не настало
                // для отдачи
                else if front_val.pop_time > Instant::now() {
                    // Создаем тогда футуру для пробуждения
                    let prev = self.as_mut().sleep_future.replace(SleepLocal::new(
                        tokio::time::sleep_until(front_val.pop_time.into()),
                    ));

                    assert!(prev.is_none(), "Sleep future should not exist");

                    // Выставляем флаг резервирования текущим футуры
                    front_val.reserved.store(self.future_id, Ordering::Release);

                    // Создаем waker для отслеживания отмены футуры
                    self.reserve_waker = Some(ReserveWaker {
                        condvar: self.reserve_condvar,
                        item_reserved_future: Arc::downgrade(&front_val.reserved),
                    });

                    drop(lock);

                    // Запустим футуру ожидания на новой итерации
                    continue 'main_loop;
                }
                // Можно отдавать прямо сейчас, условия все соблюдены
                else {
                    // Теперь можем смело извлечиь итем, он там точно есть - проверка выше,
                    // поэтому можно unwrap
                    let item = lock.pop_front().expect("First item should exist").item;

                    // Перед уведомлением снимаем блокировку
                    drop(lock);

                    // Говорим, что освободилось новое место
                    self.size_condvar.notify_one();

                    // Дополнительно можно было бы еще уведомить об этом через reserve_condvar,
                    // но это будет сделано автоматически при уничтожении футуры.
                    // self.reserve_condvar.notify_one();

                    // Итем готов
                    return Poll::Ready(item);
                }
            } else {
                // Создаем футуру ожидания
                let wait_future = self.size_condvar.wait_no_relock(lock);

                // Раз блокировка каждый раз новая, то и футура для
                // просыпания тоже пусть будет новая каждый раз
                let prev = self.as_mut().condvar_future.replace(Box::pin(wait_future));

                // Прошлой футуры быть не должно здесь
                assert!(prev.is_none(), "Previous condvar wait should not exist");

                // Полить будем на следующей итерации
                continue 'main_loop;
            }
        }
    }
}

// // Если нотифаера не было еще - регистрируем его.
// let notified = unsafe {
//     // Создаем ссылкe только на notify, без self
//     let notify_ref = self.condvar;

//     // Создаем новый Pin c помощью as_mut из текущего
//     let new_pin = self.as_mut();

//     // Затем получаем мутабельную ссылку из нового Pin
//     let mut_ref: &mut Self = new_pin.get_unchecked_mut();

//     // Теперь по этой ссылке мы можем создать или получить футуру
//     mut_ref
//         .notified
//         .get_or_insert_with(move || notify_ref.wait_no_relock(lock))
// };

// Снимаем блокировку перед ожиданием уведомлений, нотифаер был создан заранее,
// поэтому вроде бы должно работать все как со стандартным condvar:
// - регистрируем поддержку
// - снимаем блокировку
// - ждем
// drop(lock);

// let notified_pinned = unsafe { Pin::new_unchecked(notified) };

// Получаем проекцию на запинированный нотифаер
/* let notified_pinned = unsafe {
    // Создаем новый Pin c помощью as_mut из текущего
    let new_pin = self.as_mut();
    // Затем получаем мутабельную ссылку из нового Pin
    let mut_ref: &mut Self = new_pin.get_unchecked_mut();
    // Теперь создаем project уже
    Pin::new_unchecked(&mut notified)
}; */

// Альтернативный вариант
// let pinned = unsafe { self.as_mut().map_unchecked_mut(|v| &mut v.notified) };

// Проверяем разово готовность данного нотифаера, может быть уже стало что-то готово?
// При пробуждении мы проверим работу еще раз
// ready!(notified_pinned.poll(cx));
