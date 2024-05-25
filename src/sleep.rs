use std::{future::Future, pin::Pin, task::Poll};
use tokio::time::Sleep;

////////////////////////////////////////////////////////////////////////////////

/// Обертка над футурой сна
pub(super) struct SleepLocal(Sleep);

impl SleepLocal {
    pub(super) fn new(s: Sleep) -> SleepLocal {
        SleepLocal(s)
    }
}

// Помечаем, что эта самая футура не привязана к расположению в памяти
// TODO: Но вроде бы это и так у нас будет автоматически?
impl Unpin for SleepLocal {}

impl Future for SleepLocal {
    type Output = <Sleep as Future>::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        // Здесь никак иначе, но мы знаем, что текущая футура у нас Unpin,
        // значит можно создать пинированную ссылку на чилда
        // TODO: Либо можно взять библиотеку pin_project, она делает то же самое
        let projection = unsafe { Pin::new_unchecked(&mut self.get_mut().0) };

        // Полим чилда
        projection.poll(cx)
    }
}
