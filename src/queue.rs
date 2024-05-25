////////////////////////////////////////////////////////////////////////////////

//
// Еще можно сделать просто свою футуру-обертку, которая будет
// в себя включать очередь + футуру задержки работы
// Есть уже кое-какая альтернатива:
// - https://crates.io/crates/futures-delay-queue
// - https://github.com/bells307/throttled-stream
// - https://crates.io/crates/stream_throttle
// - https://crates.io/crates/delay-queue
// - https://crates.io/crates/deadqueue

////////////////////////////////////////////////////////////////////////////////

use crate::{future::DelayedPopFuture, item::DelayItem};
use async_condvar_fair::{BatonExt, Condvar};
use parking_lot::Mutex;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

////////////////////////////////////////////////////////////////////////////////

/// Структура данных, которую шарим между потоков
struct Inner<T> {
    /// Максимальный размер очереди
    max_size: usize,

    /// Очередь с синхронной блокировкой
    queue: Mutex<VecDeque<DelayItem<T>>>,

    /// Асинхронный condvar для оповещения об изменениях
    size_condvar: Condvar,

    /// Асинхронный condvar для оповещения об изменениях
    reserve_condvar: Condvar,

    /// Счетчик футур ожидания
    counter: AtomicU64,
}

////////////////////////////////////////////////////////////////////////////////

/// Delayed queue with atomic pop with cancellation support.
///
/// ```rust
/// # use tokio_delayed_queue::DelayedQueue;
/// # use std::time::Duration;
/// # tokio_test::block_on(async {
/// 
/// // New queue
/// let queue = DelayedQueue::new(16);
/// 
/// // Push
/// queue.push(1, Duration::from_secs(1)).await;
/// 
/// // Pop
/// let v = queue.pop().await;
/// assert_eq!(v, 1);
/// 
/// # });
/// ```
//
// Используем `derive_where`,
// чтобы не накладывать дополнительные условия на тип `T`.
// #[derive_where(Clone)]
pub struct DelayedQueue<T> {
    inner: Arc<Inner<T>>,
}

impl<T> DelayedQueue<T> {
    /// Creates new queue with fixed preallocated capacity.
    // Создание очереди сразу нужной емкости, аллоцируем сразу же нужный размер один раз.
    pub fn new(size: usize) -> DelayedQueue<T> {
        DelayedQueue {
            inner: Arc::new(Inner {
                max_size: size,
                queue: Mutex::new(VecDeque::with_capacity(size)),
                size_condvar: Condvar::new(),
                reserve_condvar: Condvar::new(),
                counter: AtomicU64::new(1),
            }),
        }
    }

    /// Push new item.
    // Добавляем новый итем с задержкой
    #[allow(clippy::await_holding_lock)]
    pub async fn push(&self, item: T, delay: Duration) {
        // Для удобства
        let this = self.inner.as_ref();

        // Когда будем пробуждаться
        let pop_time = Instant::now() + delay;

        // Пробуем получить блокировку над очередью
        let mut lock = loop {
            // Берем блокировку короткую над очередью
            let lock = this.queue.lock();

            // Проверяем размер очереди, если превышен
            if lock.len() >= this.max_size {
                // Тогда подождем возможности запихнуть новый элемент
                this.size_condvar.wait_no_relock(lock).await.dispose();
            } else {
                // Все норм - отдаем дальше блокировку
                break lock;
            }
        };

        // Учитываем в том числе время нахождения в очереди,
        // поэтому создаем фиксированнную точку пробуждения
        // let pop_time = tokio::time::Instant::now() + delay;

        // Новый итем, сразу с футурой ожидания
        let queue_item = DelayItem {
            pop_time,
            item,
            reserved: Arc::new(AtomicU64::new(0)),
        };

        // Добавляем итем
        lock.push_back(queue_item);

        // Снимаем блокировку
        drop(lock);

        // Теперь уведомляем, что итем стал доступен новый
        this.size_condvar.notify_one();
    }

    /// Atomically pop delayed item. It supports pop cancelation by returned future drop.
    // Получение нового итема с нужной задержкой
    pub fn pop(&self) -> DelayedPopFuture<T> {
        DelayedPopFuture {
            queue: &self.inner.queue,
            size_condvar: &self.inner.size_condvar,
            reserve_condvar: &self.inner.reserve_condvar,
            condvar_future: None,
            sleep_future: None,
            reserve_waker: None,
            future_id: self.inner.counter.fetch_add(1, Ordering::Release),
        }
    }
}

impl<T> Clone for DelayedQueue<T> {
    fn clone(&self) -> Self {
        DelayedQueue {
            inner: self.inner.clone(),
        }
    }
}
