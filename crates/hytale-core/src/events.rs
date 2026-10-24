//! Typed event bus with ordered synchronous and async subscribers.
//!
//! `publish` serializes delivery through one mutex, so every subscriber sees
//! events in publish order even when publishers run on different threads.
//! Synchronous callbacks are invoked under that lock; they must not call
//! `publish` again (use the async seam for work that needs to publish).
//!
//! Async subscribers get a bounded Tokio channel. When it is full the event is
//! dropped for that subscriber and counted in [`PublishReport::dropped`];
//! ordering is never sacrificed for buffering.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, Weak};

use tokio::sync::mpsc;

/// Result of one publish.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PublishReport {
    /// Subscribers registered at publish time.
    pub subscribers: usize,
    /// Subscribers that accepted the event.
    pub delivered: usize,
    /// Async subscribers whose channel was full (or closed).
    pub dropped: usize,
}

/// A subscription handle. Dropping it unsubscribes.
pub struct Subscription<E: Send + Sync + 'static> {
    inner: Weak<BusInner<E>>,
    id: u64,
    name: String,
}

impl<E: Send + Sync + 'static> Subscription<E> {
    /// Subscriber id (monotonic within one bus).
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Subscriber name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl<E: Send + Sync + 'static> Drop for Subscription<E> {
    fn drop(&mut self) {
        let Some(inner) = self.inner.upgrade() else {
            return;
        };
        if let Ok(mut state) = inner.state.lock() {
            state.subscribers.remove(&self.id);
        };
    }
}

impl<E: Send + Sync + 'static> std::fmt::Debug for Subscription<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Subscription")
            .field("id", &self.id)
            .field("name", &self.name)
            .finish()
    }
}

struct BusInner<E: Send + Sync + 'static> {
    state: Mutex<State<E>>,
}

struct State<E: Send + Sync + 'static> {
    next_id: u64,
    subscribers: BTreeMap<u64, Subscriber<E>>,
}

enum Subscriber<E: Send + Sync + 'static> {
    Sync {
        name: String,
        callback: Arc<dyn Fn(&E) + Send + Sync>,
    },
    Async {
        name: String,
        sender: mpsc::Sender<Arc<E>>,
    },
}

impl<E: Send + Sync + 'static> Subscriber<E> {
    fn name(&self) -> &str {
        match self {
            Self::Sync { name, .. } | Self::Async { name, .. } => name,
        }
    }
}

/// A typed event bus.
pub struct EventBus<E: Send + Sync + 'static> {
    inner: Arc<BusInner<E>>,
}

impl<E: Send + Sync + 'static> EventBus<E> {
    /// Create an empty bus.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(BusInner {
                state: Mutex::new(State {
                    next_id: 0,
                    subscribers: BTreeMap::new(),
                }),
            }),
        }
    }

    /// Register a synchronous callback. The callback runs during `publish`.
    pub fn subscribe_sync<F>(&self, name: impl Into<String>, callback: F) -> Subscription<E>
    where
        F: Fn(&E) + Send + Sync + 'static,
    {
        self.insert(Subscriber::Sync {
            name: name.into(),
            callback: Arc::new(callback),
        })
    }

    /// Register an async subscriber and return its ordered receiver.
    ///
    /// `capacity` is clamped to at least one.
    pub fn subscribe_async(
        &self,
        name: impl Into<String>,
        capacity: usize,
    ) -> (Subscription<E>, mpsc::Receiver<Arc<E>>) {
        let (sender, receiver) = mpsc::channel(capacity.max(1));
        let subscription = self.insert(Subscriber::Async {
            name: name.into(),
            sender,
        });
        (subscription, receiver)
    }

    /// Number of currently registered subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.lock().subscribers.len()
    }

    /// Publish one event to every subscriber in subscription order.
    pub fn publish(&self, event: E) -> PublishReport {
        let event = Arc::new(event);
        let mut state = self.lock();
        let subscribers = state.subscribers.len();
        let mut report = PublishReport {
            subscribers,
            ..PublishReport::default()
        };
        let mut dead = Vec::new();

        for (id, subscriber) in &mut state.subscribers {
            match subscriber {
                Subscriber::Sync { callback, .. } => {
                    callback(&event);
                    report.delivered += 1;
                }
                Subscriber::Async { sender, .. } => match sender.try_send(Arc::clone(&event)) {
                    Ok(()) => report.delivered += 1,
                    Err(mpsc::error::TrySendError::Full(_)) => report.dropped += 1,
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        report.dropped += 1;
                        dead.push(*id);
                    }
                },
            }
        }
        for id in dead {
            state.subscribers.remove(&id);
        }
        report
    }

    /// Names of the registered subscribers, in delivery order.
    pub fn subscriber_names(&self) -> Vec<String> {
        self.lock()
            .subscribers
            .values()
            .map(|subscriber| subscriber.name().to_string())
            .collect()
    }

    fn insert(&self, subscriber: Subscriber<E>) -> Subscription<E> {
        let mut state = self.lock();
        let id = state.next_id;
        state.next_id += 1;
        let name = subscriber.name().to_string();
        state.subscribers.insert(id, subscriber);
        Subscription {
            inner: Arc::downgrade(&self.inner),
            id,
            name,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State<E>> {
        self.inner.state.lock().expect("event bus mutex poisoned")
    }
}

impl<E: Send + Sync + 'static> Default for EventBus<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: Send + Sync + 'static> std::fmt::Debug for EventBus<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EventBus")
            .field("subscribers", &self.subscriber_names())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Number(u32);

    #[test]
    fn synchronous_subscribers_see_publish_order() {
        let bus = EventBus::new();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let _subscription = bus.subscribe_sync("sync", move |event: &Number| {
            sink.lock().unwrap().push(event.0);
        });
        for value in 0..8 {
            let report = bus.publish(Number(value));
            assert_eq!(report.delivered, 1);
        }
        assert_eq!(*seen.lock().unwrap(), vec![0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[tokio::test]
    async fn async_subscribers_see_publish_order_independently() {
        let bus = EventBus::new();
        let (_first_sub, mut first) = bus.subscribe_async("first", 16);
        let (_second_sub, mut second) = bus.subscribe_async("second", 16);
        for value in 0..8 {
            bus.publish(Number(value));
        }
        for expected in 0..8 {
            assert_eq!(first.recv().await.unwrap().0, expected);
            assert_eq!(second.recv().await.unwrap().0, expected);
        }
    }

    #[tokio::test]
    async fn dropping_a_subscription_stops_delivery() {
        let bus = EventBus::new();
        let seen = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&seen);
        let subscription = bus.subscribe_sync("sync", move |_: &Number| {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        bus.publish(Number(1));
        drop(subscription);
        assert_eq!(bus.subscriber_count(), 0);
        assert_eq!(bus.publish(Number(2)).delivered, 0);
        assert_eq!(seen.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn full_async_channels_drop_without_reordering() {
        let bus = EventBus::new();
        let (_subscription, mut receiver) = bus.subscribe_async("slow", 1);
        assert_eq!(bus.publish(Number(1)).delivered, 1);
        let report = bus.publish(Number(2));
        assert_eq!(report.dropped, 1);
        assert_eq!(receiver.recv().await.unwrap().0, 1);
    }

    #[tokio::test]
    async fn closed_async_channel_is_removed() {
        let bus = EventBus::new();
        let (subscription, receiver) = bus.subscribe_async("gone", 1);
        drop(receiver);
        assert_eq!(bus.publish(Number(1)).dropped, 1);
        assert_eq!(bus.subscriber_count(), 0);
        drop(subscription);
    }

    #[test]
    fn subscriber_names_follow_registration_order() {
        let bus = EventBus::new();
        let _a = bus.subscribe_sync("alpha", |_: &Number| {});
        let _b = bus.subscribe_sync("beta", |_: &Number| {});
        assert_eq!(bus.subscriber_names(), vec!["alpha", "beta"]);
    }
}
