use chrono::{DateTime, Datelike, NaiveDate, Timelike, Utc};
use kagura_support::Span;
use kagura_value::Value;

use crate::error::RuntimeError;

use super::super::Interpreter;

pub(super) fn now_millis() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as f64)
        .unwrap_or(f64::NAN)
}

pub(super) fn parse(text: &str) -> f64 {
    if let Ok(value) = DateTime::parse_from_rfc3339(text) {
        return value.timestamp_millis() as f64;
    }
    if let Ok(value) = NaiveDate::parse_from_str(text, "%Y-%m-%d")
        && let Some(value) = value.and_hms_opt(0, 0, 0)
    {
        return value.and_utc().timestamp_millis() as f64;
    }
    f64::NAN
}

fn components(args: &[Value], heap: &kagura_value::Heap) -> f64 {
    let number = |index: usize, default: f64| {
        args.get(index)
            .and_then(|value| heap.to_number(*value))
            .unwrap_or(default)
    };
    let year = number(0, f64::NAN);
    let month = number(1, 0.0);
    let day = number(2, 1.0);
    let hour = number(3, 0.0);
    let minute = number(4, 0.0);
    let second = number(5, 0.0);
    let millisecond = number(6, 0.0);
    if !year.is_finite()
        || !month.is_finite()
        || !day.is_finite()
        || !hour.is_finite()
        || !minute.is_finite()
        || !second.is_finite()
        || !millisecond.is_finite()
    {
        return f64::NAN;
    }
    let year = year as i32;
    let month = month as i32 + 1;
    let day = day as i32;
    let Some(date) = NaiveDate::from_ymd_opt(year, month as u32, day as u32) else {
        return f64::NAN;
    };
    let Some(time) = date.and_hms_milli_opt(
        hour as u32,
        minute as u32,
        second as u32,
        millisecond as u32,
    ) else {
        return f64::NAN;
    };
    time.and_utc().timestamp_millis() as f64
}

pub(super) fn utc(args: &[Value], heap: &kagura_value::Heap) -> f64 {
    components(args, heap)
}

pub(super) fn format_iso(millis: f64) -> String {
    DateTime::<Utc>::from_timestamp_millis(millis as i64)
        .map(|value| value.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
        .unwrap_or_else(|| "Invalid Date".into())
}

impl Interpreter<'_> {
    pub(super) fn date_construct(
        &mut self,
        args: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let millis = match args {
            [] => now_millis(),
            [value] => {
                if let Some(text) = self.heap.as_str(*value) {
                    parse(text)
                } else if value.is_undefined() {
                    f64::NAN
                } else {
                    self.heap.to_number(*value).unwrap_or(f64::NAN)
                }
            }
            _ => components(args, self.heap),
        };
        let object = self
            .objects
            .allocate(self.shapes.root())
            .map_err(|error| RuntimeError::runtime(error.to_string(), span))?;
        let object = Value::object(object.index());
        self.set_property(object, "__date", Value::boolean(true), span)?;
        let millis = self.heap.number(millis);
        self.set_property(object, "__time", millis, span)?;
        Ok(object)
    }

    pub(super) fn call_date_method(
        &mut self,
        object: Value,
        key: &str,
        args: &[Value],
        span: Span,
    ) -> Result<Option<Value>, RuntimeError> {
        let millis = self.get_property(object, "__time", span)?;
        let millis = self.heap.as_number(millis).unwrap_or(f64::NAN);
        if key == "setTime" {
            let value = args
                .first()
                .and_then(|value| self.heap.to_number(*value))
                .unwrap_or(f64::NAN);
            let value = self.heap.number(value);
            self.set_property(object, "__time", value, span)?;
            return Ok(Some(value));
        }
        if key == "getTime" || key == "valueOf" {
            return Ok(Some(self.heap.number(millis)));
        }
        if millis.is_nan() {
            if key == "toISOString" {
                return Err(RuntimeError::range("Invalid time value", span));
            }
            return Ok(Some(self.heap.number(f64::NAN)));
        }
        let Some(value) = DateTime::<Utc>::from_timestamp_millis(millis as i64) else {
            return Ok(Some(self.heap.number(f64::NAN)));
        };
        let result = match key {
            "toISOString" => {
                return Ok(Some(
                    self.heap
                        .string(&value.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()),
                ));
            }
            "toJSON" => {
                return Ok(Some(
                    self.heap
                        .string(&value.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()),
                ));
            }
            "toString" => {
                return Ok(Some(
                    self.heap.string(
                        &value
                            .format("%a %b %d %Y %H:%M:%S GMT+0000 (UTC)")
                            .to_string(),
                    ),
                ));
            }
            "getUTCFullYear" | "getFullYear" => value.year() as f64,
            "getUTCMonth" | "getMonth" => value.month0() as f64,
            "getUTCDate" | "getDate" => value.day() as f64,
            "getUTCDay" | "getDay" => value.weekday().num_days_from_sunday() as f64,
            "getUTCHours" | "getHours" => value.hour() as f64,
            "getUTCMinutes" | "getMinutes" => value.minute() as f64,
            "getUTCSeconds" | "getSeconds" => value.second() as f64,
            "getUTCMilliseconds" | "getMilliseconds" => value.nanosecond() as f64 / 1_000_000.0,
            "getYear" => (value.year() - 1900) as f64,
            "getTimezoneOffset" => 0.0,
            _ => return Ok(None),
        };
        Ok(Some(self.heap.number(result)))
    }
}
