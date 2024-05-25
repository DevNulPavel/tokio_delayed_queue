use super::*;
use std::time::Duration;

#[tokio::test]
async fn test_func() {
    let queue = DelayedQueue::new(16);
    queue.push(1, Duration::from_secs(1)).await;
    queue.push(1, Duration::from_secs(2)).await;

    let v = queue.pop().await;
    assert_eq!(v, 1);

    let j1 = tokio::spawn({
        let queue = queue.clone();
        async move {
            let dropped_future = queue.pop();
            drop(dropped_future);

            let dropped_future = queue.pop();
            drop(dropped_future);

            let v = queue.pop().await;
            assert_eq!(v, 1);
        }
    });

    let j2 = tokio::spawn({
        let queue = queue.clone();
        async move {
            let dropped_future = queue.pop();
            drop(dropped_future);

            let v = queue.pop().await;
            assert_eq!(v, 1);
        }
    });

    let j3 = tokio::spawn({
        let queue = queue.clone();
        async move {
            let v = queue.pop().await;
            assert_eq!(v, 1);
        }
    });

    queue.push(1, Duration::from_secs(3)).await;
    queue.push(1, Duration::from_secs(4)).await;
    queue.push(1, Duration::from_secs(4)).await;

    j1.await.unwrap();
    j2.await.unwrap();
    j3.await.unwrap();

    let v = queue.pop().await;
    assert_eq!(v, 1);
}
