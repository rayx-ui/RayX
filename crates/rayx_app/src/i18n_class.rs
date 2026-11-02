//! The i18n service, for TSX.

use std::cell::RefCell;
use std::collections::HashMap;

use std::str::FromStr;

use dxn_intl::{
    DateFields, DateTimeOptions, Grouping, HourCycle, Length, Number, NumberOptions, NumberStyle,
    TimePrecision,
};
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
/// resolved, then `i18n.t("cart", { n: 3 })`, `i18n.formatDate(date.toISOString())` and
/// `i18n.formatNumber(0.25, { style: "percent" })`.
///
/// Arguments are a plain object: numbers format as numbers (and select plurals), strings as
/// text, booleans as `true`/`false`, and an instant (`date.toISOString()`: a timestamp with its
/// offset) as a date in the user's time zone. A `Date` or any other object throws: JavaScript
/// hands it over as an empty object. `onChange` hears every change of the locale, the time zone
/// or the messages, with the locale's tag; format again then.
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

    /// The message `id` formatted with `args`; throws for an argument that is an object.
    pub fn t(&self, id: String, args: Option<JsonValue>) -> Result<String, String> {
        Ok(self.service.format(&id, &arguments(&self.service, args)?))
    }

    /// `instant` (`date.toISOString()`: a timestamp with its offset) as the user's wall time in
    /// their time zone, formatted for the locale (by default its date at medium length).
    pub fn format_date(
        &self,
        instant: String,
        options: Option<DateFormat>,
    ) -> Result<String, String> {
        let local = self
            .service
            .local_time(&instant)
            .map_err(|error| error.to_string())?;
        self.service
            .format_date_time(&local, options.unwrap_or_default().resolve()?)
            .map_err(|error| error.to_string())
    }

    /// `value` formatted for the locale (by default as a decimal number).
    pub fn format_number(
        &self,
        value: f64,
        options: Option<NumberFormat>,
    ) -> Result<String, String> {
        let number = Number::try_from(value).map_err(|error| error.to_string())?;
        self.service
            .format_number(number, options.unwrap_or_default().resolve()?)
            .map_err(|error| error.to_string())
    }

    /// The user's time zone (an IANA name such as `Europe/Zagreb`), which instants are shown in.
    pub fn time_zone(&self) -> String {
        self.service.time_zone()
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

/// The length of [`DateFormat`]'s text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PropEnum)]
pub enum DateLength {
    /// `11/2/26, 10:30 AM`.
    Short,
    /// `Nov 2, 2026, 10:30 AM`.
    #[default]
    Medium,
    /// `November 2, 2026 at 10:30 AM`.
    Long,
}

/// How precisely [`DateFormat`] shows the time of day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PropEnum)]
pub enum TimeDetail {
    /// `10 AM`.
    Hour,
    /// `10:30 AM`.
    Minute,
    /// `10:30:15 AM`.
    Second,
}

/// How `I18n.formatDate` shows an instant.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct DateFormat {
    /// The text's length (default `"medium"`).
    #[prop(default)]
    pub length: DateLength,
    /// Whether the date is shown (default: yes, unless only `time` is asked for with
    /// `date: false`).
    pub date: Option<bool>,
    /// The time of day, to this precision (default: none).
    pub time: Option<TimeDetail>,
    /// The clock, `"h11"`, `"h12"`, `"h23"` or `"h24"` (default: the locale's and the user's).
    pub hour_cycle: Option<String>,
}

impl DateFormat {
    fn resolve(self) -> Result<DateTimeOptions, String> {
        let length = match self.length {
            DateLength::Short => Length::Short,
            DateLength::Medium => Length::Medium,
            DateLength::Long => Length::Long,
        };
        let time_precision = self.time.map(|detail| match detail {
            TimeDetail::Hour => TimePrecision::Hour,
            TimeDetail::Minute => TimePrecision::Minute,
            TimeDetail::Second => TimePrecision::Second,
        });
        let date_fields = self
            .date
            .unwrap_or(true)
            .then_some(DateFields::YearMonthDay);
        if date_fields.is_none() && time_precision.is_none() {
            return Err("formatDate shows nothing with `date: false` and no `time`".into());
        }
        let hour_cycle = self
            .hour_cycle
            .map(|cycle| HourCycle::from_str(&cycle).map_err(|error| error.to_string()))
            .transpose()?;
        Ok(DateTimeOptions {
            date_fields,
            time_precision,
            length,
            hour_cycle,
        })
    }
}

/// What a number stands for in [`NumberFormat`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PropEnum)]
pub enum NumberKind {
    /// `1,234.5`.
    #[default]
    Decimal,
    /// A fraction as a percentage: `0.25` is `25%`.
    Percent,
    /// An amount of money in `currency`: `€1,234.50`.
    Currency,
}

/// How `I18n.formatNumber` shows a number.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub struct NumberFormat {
    /// Decimal, percent or currency (default `"decimal"`).
    #[prop(default)]
    pub style: NumberKind,
    /// The ISO 4217 code with `style: "currency"` (`"EUR"`).
    pub currency: Option<String>,
    /// Leading zeros pad the integer part to this many digits (default 1).
    pub minimum_integer_digits: Option<u32>,
    /// Trailing zeros pad the fraction to this many digits (default 0).
    pub minimum_fraction_digits: Option<u32>,
    /// The fraction is rounded to this many digits (default 3, 0 for percentages).
    pub maximum_fraction_digits: Option<u32>,
    /// Grouping separators as the locale has them (default), or none with `false`.
    pub use_grouping: Option<bool>,
}

impl NumberFormat {
    fn resolve(self) -> Result<NumberOptions, String> {
        let digits = |value: Option<u32>, what: &str| {
            value
                .map(|digits| {
                    u8::try_from(digits).map_err(|_| format!("{what} {digits} is too many"))
                })
                .transpose()
        };
        let style = match (self.style, self.currency) {
            (NumberKind::Currency, Some(code)) => NumberStyle::Currency(code),
            (NumberKind::Currency, None) => {
                return Err("formatNumber's `style: \"currency\"` needs a `currency`".into());
            }
            (NumberKind::Percent, _) => NumberStyle::Percent,
            (NumberKind::Decimal, _) => NumberStyle::Decimal,
        };
        Ok(NumberOptions {
            style,
            minimum_integer_digits: digits(self.minimum_integer_digits, "minimumIntegerDigits")?
                .unwrap_or(1),
            minimum_fraction_digits: digits(self.minimum_fraction_digits, "minimumFractionDigits")?,
            maximum_fraction_digits: digits(self.maximum_fraction_digits, "maximumFractionDigits")?,
            grouping: match self.use_grouping {
                Some(false) => Grouping::Never,
                _ => Grouping::Auto,
            },
            ..NumberOptions::default()
        })
    }
}

/// A TSX object's fields as message arguments: an instant (a timestamp with its offset) is a
/// date in the user's time zone, and an object is refused.
fn arguments(service: &I18nService, args: Option<JsonValue>) -> Result<Args, String> {
    let fields = match args {
        None | Some(JsonValue::Null) => return Ok(Args::new()),
        Some(JsonValue::Object(fields)) => fields,
        Some(other) => {
            return Err(format!(
                "t's arguments are an object of values, not {other}"
            ));
        }
    };
    fields
        .into_iter()
        .map(|(name, value)| {
            let value = match value {
                JsonValue::String(text) => match service.local_time(&text) {
                    Ok(local) => Value::DateTime(local),
                    Err(_) => Value::String(text),
                },
                JsonValue::Number(number) => match number.as_i64() {
                    Some(integer) => Value::from(integer),
                    None => Value::from(number.as_f64().unwrap_or(f64::NAN)),
                },
                JsonValue::Bool(flag) => Value::String(flag.to_string()),
                JsonValue::Null => Value::String(String::new()),
                JsonValue::Object(_) | JsonValue::Array(_) => {
                    return Err(format!(
                        "argument `{name}` is an object: pass a Date as `date.toISOString()`, \
                         and text, numbers and booleans as they are"
                    ));
                }
            };
            Ok((name, value))
        })
        .collect()
}
