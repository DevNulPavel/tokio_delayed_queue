// #![doc = include_str!("../README.md")]

/// # Tokio delayed queue
/// 
/// Asyncronous delayed queue for Tokio runtime. 
/// 
/// # Features
/// 
/// - multi-consume
/// - multi-produce
/// - fixed queue size
/// - atomic pop with pop-future cancelation
///
/// ```rust
/// # use tokio_delayed_queue::DelayedQueue;
/// # use std::time::Duration;
/// # tokio_test::block_on(async {
/// 
/// let queue = DelayedQueue::new(16);
/// 
/// // Push
/// queue.push(1, Duration::from_secs(1)).await;
/// queue.push(1, Duration::from_secs(2)).await;
/// 
/// // Pop
/// let v = queue.pop().await;
/// assert_eq!(v, 1);
/// 
/// // Other future
/// let join = tokio::spawn({
///     let queue = queue.clone();
///     async move {
///         // Cancelled 1
///         let dropped_future = queue.pop();
///         drop(dropped_future);
/// 
///         // Cancelled 2
///         let dropped_future = queue.pop();
///         drop(dropped_future);
/// 
///         // Pop
///         let v = queue.pop().await;
///         assert_eq!(v, 1);
/// 
///         // Pop
///         let v = queue.pop().await;
///         assert_eq!(v, 1);
///     }
/// });
/// 
/// // Push
/// queue.push(1, Duration::from_secs(2)).await;
/// 
/// join.await.unwrap();
/// 
/// # });
/// ```

////////////////////////////////////////////////////////////////////////////////////////////////////

mod future;
mod item;
mod queue;
mod reserve;
mod sleep;

////////////////////////////////////////////////////////////////////////////////////////////////////

pub use self::{future::DelayedPopFuture, queue::DelayedQueue};
