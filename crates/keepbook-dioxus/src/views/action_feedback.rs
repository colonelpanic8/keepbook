use super::*;

const DONE_VISIBLE_MS: u64 = 3_000;

/// The state of the work a view's controls started, keyed by the control
/// that started it.
///
/// Views hand [`ActionFeedback::for_key`] to that control's `feedback` (or
/// [`ActionFeedback::status_for`] to a panel's `status`). Successes clear
/// themselves. A failure stays, with [`ActionFeedback::error`] shown in an
/// `ErrorNotice` beneath the control, until it is dismissed or the work is
/// tried again.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct ActionFeedback {
    state: Signal<Option<(String, ButtonFeedback)>>,
    generation: Signal<u64>,
}

pub(super) fn use_action_feedback() -> ActionFeedback {
    ActionFeedback {
        state: use_signal(|| None),
        generation: use_signal(|| 0),
    }
}

impl ActionFeedback {
    pub(super) fn start(self, key: impl Into<String>, label: impl Into<String>) {
        self.set(key.into(), FeedbackTone::Busy, label.into(), None);
    }

    pub(super) fn succeed(
        self,
        key: impl Into<String>,
        label: impl Into<String>,
        detail: Option<String>,
    ) {
        self.set(key.into(), FeedbackTone::Done, label.into(), detail);
    }

    pub(super) fn fail(self, key: impl Into<String>, label: impl Into<String>, detail: String) {
        self.set(key.into(), FeedbackTone::Failed, label.into(), Some(detail));
    }

    /// Feedback for whichever control has some.
    pub(super) fn current(self) -> Option<ButtonFeedback> {
        self.state
            .read()
            .as_ref()
            .map(|(_, feedback)| feedback.clone())
    }

    pub(super) fn for_key(self, key: &str) -> Option<ButtonFeedback> {
        self.state
            .read()
            .as_ref()
            .filter(|(owner, _)| owner == key)
            .map(|(_, feedback)| feedback.clone())
    }

    pub(super) fn is_busy(self) -> bool {
        self.current()
            .is_some_and(|feedback| feedback.tone == FeedbackTone::Busy)
    }

    /// One line for a panel's `status`. Failures show in the panel's
    /// `ErrorNotice` instead.
    pub(super) fn status_for(self, key: &str) -> Option<String> {
        self.for_key(key)
            .filter(|feedback| feedback.tone != FeedbackTone::Failed)
            .map(|feedback| feedback.label)
    }

    /// The full message of a failure that hasn't been dismissed.
    pub(super) fn error(self) -> Option<String> {
        self.current()
            .filter(|feedback| feedback.tone == FeedbackTone::Failed)
            .map(|feedback| feedback.detail.unwrap_or(feedback.label))
    }

    pub(super) fn error_for(self, key: &str) -> Option<String> {
        self.for_key(key).and(self.error())
    }

    pub(super) fn dismiss(mut self) {
        self.generation += 1;
        self.state.set(None);
    }

    fn set(mut self, key: String, tone: FeedbackTone, label: String, detail: Option<String>) {
        self.state.set(Some((
            key,
            ButtonFeedback {
                tone,
                label,
                detail,
            },
        )));
        let generation = *self.generation.peek() + 1;
        self.generation.set(generation);
        if tone != FeedbackTone::Done {
            return;
        }
        spawn(async move {
            sleep_ms(DONE_VISIBLE_MS).await;
            if self
                .generation
                .try_peek()
                .is_ok_and(|current| *current == generation)
            {
                self.state.set(None);
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
