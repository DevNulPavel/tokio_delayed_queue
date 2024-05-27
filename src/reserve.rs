use async_condvar_fair::Condvar;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Weak,
};

////////////////////////////////////////////////////////////////////////////////

pub(super) struct ReserveWaker<'a> {
    /// Пробуждалка для резервирования
    pub(super) condvar: &'a Condvar,
    
    /// Используется Arc, так как очередь у нас под блокировкой
    pub(super) item_reserved_future: Weak<AtomicU64>,
}

impl<'a> Drop for ReserveWaker<'a> {
    fn drop(&mut self) {
        // Никто больше не резервирует итем
        if let Some(still_reserved) = self.item_reserved_future.upgrade() {
            // Снимаем статус резервирования
            still_reserved.store(0, Ordering::Release);
        }

        // Всегда уведомляем кого-то, кто ждет результат, а не только при отмене.
        self.condvar.notify_one();
    }
}
