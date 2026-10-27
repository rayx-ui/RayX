//! The i18n service, for TSX.

use std::cell::RefCell;
use std::collections::HashMap;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_core::ChangeSubscription;
use rayx_runtime::services::i18n::{Args, I18nService, Value};

use crate::{AppError, AppServices};

thread_local! {
    /// The TSX listeners of `I18n.onChange`, by id: the service's listeners must be `Send`, a
    /// TSX function is not, and both live on the engine's thread.
    static LISTENERS: RefCell<HashMap<u64, Callback<String>>> = RefCell::new(HashMap::new());
    static NEXT_ID: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// The app's translations, for TSX: `const i18n = I18n.of(app)` once `AppServices.load()`
/// resolved, then `i18n.t("cart", { n: 3 })`.
///
/// Arguments are a plain object: numbers format as numbers (and select plurals), strings as
/// text, booleans as `true`/`false`. `onChange` hears every change of the locale or the
/// messages, with the new locale's tag; format again then.
#[class]
pub struct I18n {
    service: I18nService,
    subscriptions: Vec<(u64, ChangeSubscription)>,
}

#[class]
impl I18n {
    /// The app's i18n service (`add_i18n` registers it).
    pub fn of(app: Instance<AppServices>) -> Result<Instance<I18n>, AppError> {
        let service = app.with(|app| app.services().required_service::<I18nService>())?;
        Ok(Instance::new(I18n {
            service: (*service).clone(),
            subscriptions: Vec::new(),
        }))
    }

    /// The message `id` formatted with `args`.
    pub fn t(&self, id: String, args: Option<JsonValue>) -> String {
        self.service.format(&id, &arguments(args))
    }

    /// Whether the message `id` exists in the user's languages or the fallbacks.
    pub fn has(&self, id: String) -> bool {
        self.service.contains(&id)
    }

    /// The locale messages are formatted for (`de-AT-u-hc-h23`).
    pub fn locale(&self) -> String {
        self.service.locale().to_string()
    }

    /// The languages the app has messages for.
    pub fn languages(&self) -> Vec<String> {
        self.service.available_languages()
    }

    /// `"ltr"` or `"rtl"`.
    pub fn direction(&self) -> String {
        self.service.direction().as_str().to_owned()
    }

    /// Shows the app in `language` from now on, whatever the system's languages.
    pub fn set_language(&self, language: String) -> Result<(), String> {
        self.service
            .set_language(&language)
            .map_err(|error| error.to_string())
    }

    /// Calls `listener` with the locale's tag after every change, until this object is
    /// disposed.
    pub fn on_change(&mut self, #[args(locale)] listener: Callback<String>) {
        let id = NEXT_ID.with(|next| {
            let id = next.get();
            next.set(id + 1);
            id
        });
        LISTENERS.with(|listeners| listeners.borrow_mut().insert(id, listener));
        let subscription = self.service.subscribe(move |event| {
            let locale = event.locale.to_string();
            LISTENERS.with(|listeners| {
                if let Some(listener) = listeners.borrow().get(&id) {
                    listener.post(locale);
                }
            });
        });
        self.subscriptions.push((id, subscription));
    }
}

impl I18n {
    /// The service, for Rust bridges that take an `Instance<I18n>`.
    pub fn service(&self) -> &I18nService {
        &self.service
    }
}

impl Drop for I18n {
    fn drop(&mut self) {
        LISTENERS.with(|listeners| {
            let mut listeners = listeners.borrow_mut();
            for (id, _) in &self.subscriptions {
                listeners.remove(id);
            }
        });
    }
}

/// A TSX object's fields as message arguments.
fn arguments(args: Option<JsonValue>) -> Args {
    let Some(JsonValue::Object(fields)) = args else {
        return Args::new();
    };
    fields
        .into_iter()
        .map(|(name, value)| {
            let value = match value {
                JsonValue::String(text) => Value::String(text),
                JsonValue::Number(number) => match number.as_i64() {
                    Some(integer) => Value::from(integer),
                    None => Value::from(number.as_f64().unwrap_or(f64::NAN)),
                },
                JsonValue::Bool(flag) => Value::String(flag.to_string()),
                JsonValue::Null => Value::String(String::new()),
                other => Value::String(other.to_string()),
            };
            (name, value)
        })
        .collect()
}
