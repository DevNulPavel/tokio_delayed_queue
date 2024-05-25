use async_condvar_fair::Condvar;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

////////////////////////////////////////////////////////////////////////////////

pub(super) struct ReserveWaker<'a> {
    pub(super) condvar: &'a Condvar,
    pub(super) item_reserved_future: Arc<AtomicU64>,
}

impl<'a> Drop for ReserveWaker<'a> {
    fn drop(&mut self) {
        // Никто больше не резервирует итем
        self.item_reserved_future.store(0, Ordering::Release);

        // Уведомляем об этом не только одного ожидателя
        self.condvar.notify_one();
    }
}
