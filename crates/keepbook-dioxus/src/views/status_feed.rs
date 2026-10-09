use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_STATUS_ID: AtomicU64 = AtomicU64::new(0);

const SETTLED_BASE_MS: u64 = 4_000;
const SETTLED_PER_CHAR_MS: u64 = 40;
const SETTLED_MAX_MS: u64 = 12_000;

#[derive(Clone, Debug, PartialEq)]
struct StatusEntry {
    id: u64,
    generation: u64,
    message: String,
    busy: bool,
}

/// App-wide store behind the floating `StatusStack`.
///
/// Every mounted `FloatingStatus` publishes into it. Settled messages leave
/// after a reading delay scaled to their length, which waits while the stack
/// is hovered.
#[derive(Clone, Copy)]
pub(super) struct StatusFeed {
    entries: Signal<Vec<StatusEntry>>,
    held: Signal<bool>,
}

impl StatusFeed {
    pub(super) fn provide() -> Self {
        use_context_provider(|| Self {
            entries: Signal::new(Vec::new()),
            held: Signal::new(false),
        })
    }

    fn publish(mut self, id: u64, message: String, busy: bool) {
        let generation = {
            let mut entries = self.entries.write();
            match entries.iter_mut().find(|entry| entry.id == id) {
                Some(entry) => {
                    entry.message = message.clone();
                    entry.busy = busy;
                    entry.generation += 1;
                    entry.generation
                }
                None => {
                    entries.push(StatusEntry {
                        id,
                        generation: 0,
                        message: message.clone(),
                        busy,
                    });
                    0
                }
            }
        };
        if !busy {
            self.schedule_dismiss(id, generation, &message);
        }
    }

    fn remove(mut self, id: u64) {
        if let Ok(mut entries) = self.entries.try_write() {
            entries.retain(|entry| entry.id != id);
        }
    }

    fn set_held(mut self, held: bool) {
        self.held.set(held);
        if held {
            return;
        }
        let settled = {
            let mut entries = self.entries.write();
            entries
                .iter_mut()
                .filter(|entry| !entry.busy)
                .map(|entry| {
                    entry.generation += 1;
                    (entry.id, entry.generation, entry.message.clone())
                })
                .collect::<Vec<_>>()
        };
        for (id, generation, message) in settled {
            self.schedule_dismiss(id, generation, &message);
        }
    }

    fn schedule_dismiss(mut self, id: u64, generation: u64, message: &str) {
        let delay_ms = (SETTLED_BASE_MS + SETTLED_PER_CHAR_MS * message.chars().count() as u64)
            .min(SETTLED_MAX_MS);
        spawn(async move {
            sleep_ms(delay_ms).await;
            if self.held.try_read().is_ok_and(|held| *held) {
                return;
            }
            if let Ok(mut entries) = self.entries.try_write() {
                entries
                    .retain(|entry| entry.id != id || entry.generation != generation || entry.busy);
            }
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn sleep_ms(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

#[cfg(target_arch = "wasm32")]
async fn sleep_ms(ms: u64) {
    use wasm_bindgen::{JsCast, JsValue};

    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let set_timeout = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("setTimeout"))
            .ok()
            .and_then(|value| value.dyn_into::<js_sys::Function>().ok());
        if let Some(set_timeout) = set_timeout {
            let _ = set_timeout.call2(&JsValue::NULL, &resolve, &JsValue::from_f64(ms as f64));
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

/// Feedback for work the user started, shown in the floating `StatusStack`
/// for as long as this stays mounted. Renders nothing in place, so the
/// surrounding layout never moves.
#[component]
pub(super) fn FloatingStatus(message: String, busy: bool) -> Element {
    let feed = use_context::<StatusFeed>();
    let id = use_hook(|| NEXT_STATUS_ID.fetch_add(1, Ordering::Relaxed));
    use_effect(use_reactive!(|message, busy| {
        feed.publish(id, message, busy)
    }));
    use_drop(move || feed.remove(id));
    rsx! {}
}

#[component]
pub(super) fn StatusFeedHost() -> Element {
    let feed = use_context::<StatusFeed>();
    let entries = feed.entries.read().clone();

    rsx! {
        StatusStack { onhoverchange: move |held| feed.set_held(held),
            for entry in entries {
                OperationStatus {
                    key: "{entry.id}",
                    message: entry.message.clone(),
                    busy: entry.busy,
                    ondismiss: move |_| feed.remove(entry.id),
                }
            }
        }
    }
}
