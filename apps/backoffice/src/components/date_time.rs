use chrono::{Datelike, FixedOffset, NaiveDate, Utc};
use dioxus::prelude::*;

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

#[component]
pub fn DateTimePicker(
    id: String,
    label: String,
    value: String,
    on_input: EventHandler<String>,
    describedby: Option<String>,
    #[props(default)] invalid: bool,
    #[props(default)] required: bool,
    #[props(default)] disabled: bool,
) -> Element {
    let selected = selected_date(&value);
    let mut month = use_signal(|| month_start(selected.unwrap_or_else(today)));
    use_effect(use_reactive!(|value| {
        if let Some(date) = selected_date(&value) {
            month.set(month_start(date));
        }
    }));
    let visible_month = month();
    let days = calendar_days(visible_month);
    let time = value
        .split_once('T')
        .map_or("", |(_, time)| time.get(..8).unwrap_or(time))
        .to_owned();
    let date_value = value.clone();
    let today_value = value.clone();
    let time_value = value.clone();
    rsx! {
        div { class: "date-time-picker",
            label { r#for: "{id}", "{label}" }
            input { id: "{id}", r#type: "text", class: "date-time-value", value: "{value}",
                placeholder: "YYYY-MM-DDTHH:mm:ss", required, disabled,
                aria_describedby: describedby.clone(), aria_invalid: invalid,
                oninput: move |event| on_input(event.value()),
            }
            if !disabled {
                details { class: "date-time-disclosure",
                    summary { "Choose date and time" }
                    div { class: "date-time-panel",
                        div { class: "calendar-navigation",
                            button { r#type: "button", class: "ghost", aria_label: "Previous month",
                                disabled: visible_month.year() == 1 && visible_month.month() == 1,
                                onclick: move |_| month.set(shift_month(month(), -1)), "‹" }
                            label { class: "sr-only", r#for: "{id}-month", "Month" }
                            select { id: "{id}-month", value: "{visible_month.month()}",
                                onchange: move |event| {
                                    if let Ok(selected) = event.value().parse::<u32>()
                                        && let Some(date) = month().with_month(selected) { month.set(date); }
                                },
                                for (index, name) in MONTHS.iter().enumerate() {
                                    option { value: "{index + 1}", "{name}" }
                                }
                            }
                            label { class: "sr-only", r#for: "{id}-year", "Year" }
                            input { id: "{id}-year", class: "calendar-year", r#type: "number", min: "1", max: "9999", value: "{visible_month.year()}",
                                oninput: move |event| {
                                    if let Ok(year) = event.value().parse::<i32>()
                                        && (1..=9999).contains(&year)
                                        && let Some(date) = month().with_year(year) { month.set(date); }
                                },
                            }
                            button { r#type: "button", class: "ghost", aria_label: "Next month",
                                disabled: visible_month.year() == 9999 && visible_month.month() == 12,
                                onclick: move |_| month.set(shift_month(month(), 1)), "›" }
                        }
                        p { class: "sr-only", role: "status", aria_live: "polite", "{MONTHS[visible_month.month0() as usize]} {visible_month.year()}" }
                        div { class: "calendar-grid", role: "group", aria_label: "Choose a date",
                            for day in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] {
                                span { class: "calendar-weekday", aria_hidden: "true", "{day}" }
                            }
                            for (index, date) in days.into_iter().enumerate() {
                                if let Some(date) = date {
                                    button { key: "{date}", r#type: "button", class: "calendar-day",
                                        aria_label: date.format("%A, %d %B %Y").to_string(),
                                        aria_pressed: selected == Some(date),
                                        onclick: {
                                            let value = date_value.clone();
                                            move |_| on_input(replace_date(&value, date))
                                        }, "{date.day()}" }
                                } else { span { key: "blank-{index}", aria_hidden: "true" } }
                            }
                        }
                        TimePicker { id: "{id}-time", seconds: true, label: "Time · 24-hour clock", value: time,
                            describedby: describedby.clone(), invalid,
                            on_input: move |time: String| on_input(replace_time(&time_value, &time, today())),
                        }
                        div { class: "calendar-actions",
                            button { r#type: "button", class: "ghost", onclick: move |_| {
                                let date = today(); month.set(month_start(date)); on_input(replace_date(&today_value, date));
                            }, "Today" }
                            button { r#type: "button", class: "ghost", onclick: move |_| on_input(String::new()), "Clear" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn TimePicker(
    id: String,
    label: String,
    value: String,
    on_input: EventHandler<String>,
    describedby: Option<String>,
    #[props(default)] seconds: bool,
    #[props(default)] invalid: bool,
    #[props(default)] required: bool,
    #[props(default)] disabled: bool,
) -> Element {
    let (hour, minute, second) = clock_parts(&value);
    let hour_value = value.clone();
    let minute_value = value.clone();
    rsx! {
        fieldset { class: "time-picker", disabled,
            legend { "{label}" }
            div { class: if seconds { "time-picker-fields with-seconds" } else { "time-picker-fields" },
                div {
                    label { r#for: "{id}", "Hour" }
                    select { id: "{id}", value: "{hour}", required, aria_invalid: invalid, aria_describedby: describedby.clone(),
                        onchange: move |event| on_input(change_clock(&hour_value, true, &event.value())),
                        option { value: "", "Hour" }
                        for hour in 0..24 { option { value: format!("{hour:02}"), "{hour:02}" } }
                    }
                }
                span { class: "time-picker-separator", aria_hidden: "true", ":" }
                div {
                    label { r#for: "{id}-minute", "Minute" }
                    select { id: "{id}-minute", value: "{minute}", required, aria_invalid: invalid, aria_describedby: describedby.clone(),
                        onchange: move |event| on_input(change_clock(&minute_value, false, &event.value())),
                        option { value: "", "Minute" }
                        for minute in 0..60 { option { value: format!("{minute:02}"), "{minute:02}" } }
                    }
                }
                if seconds {
                    span { class: "time-picker-separator", aria_hidden: "true", ":" }
                    div {
                        label { r#for: "{id}-second", "Second" }
                        select { id: "{id}-second", value: "{second}", required, aria_invalid: invalid, aria_describedby: describedby.clone(),
                            onchange: move |event| on_input(change_second(&value, &event.value())),
                            option { value: "", "Second" }
                            for second in 0..60 { option { value: format!("{second:02}"), "{second:02}" } }
                        }
                    }
                }
            }
        }
    }
}

fn today() -> NaiveDate {
    Utc::now()
        .with_timezone(&FixedOffset::east_opt(7 * 3600).expect("Bangkok offset"))
        .date_naive()
}

fn selected_date(value: &str) -> Option<NaiveDate> {
    let (date, _) = value.split_once('T')?;
    NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
}

fn month_start(date: NaiveDate) -> NaiveDate {
    date.with_day(1).expect("first day")
}

fn shift_month(date: NaiveDate, change: i32) -> NaiveDate {
    let index = date.year() * 12 + date.month0().cast_signed() + change;
    NaiveDate::from_ymd_opt(index.div_euclid(12), index.rem_euclid(12) as u32 + 1, 1)
        .filter(|date| (1..=9999).contains(&date.year()))
        .unwrap_or(date)
}

fn calendar_days(date: NaiveDate) -> Vec<Option<NaiveDate>> {
    let first = month_start(date);
    let leading = first.weekday().num_days_from_monday() as usize;
    let mut cells = vec![None; leading];
    for day in 1..=31 {
        if let Some(date) = first.with_day(day) {
            cells.push(Some(date));
        }
    }
    cells.resize(cells.len().div_ceil(7) * 7, None);
    cells
}

fn replace_date(value: &str, date: NaiveDate) -> String {
    let time = value.split_once('T').map_or("00:00", |(_, time)| time);
    format!("{}T{time}", date.format("%Y-%m-%d"))
}

fn replace_time(value: &str, time: &str, fallback: NaiveDate) -> String {
    if time.is_empty() {
        return String::new();
    }
    let date = value.split_once('T').map_or_else(
        || fallback.format("%Y-%m-%d").to_string(),
        |(date, _)| date.to_owned(),
    );
    let precision = value
        .split_once('T')
        .and_then(|(_, previous)| previous.get(time.len()..))
        .unwrap_or("");
    format!("{date}T{time}{precision}")
}

fn clock_parts(value: &str) -> (String, String, String) {
    let mut parts = value.split(':');
    let hour = parts.next().unwrap_or("");
    let minute = parts.next().unwrap_or("");
    let second = parts.next().unwrap_or("").split('.').next().unwrap_or("");
    (hour.to_owned(), minute.to_owned(), second.to_owned())
}

fn change_clock(value: &str, hour_changed: bool, replacement: &str) -> String {
    if replacement.is_empty() {
        return String::new();
    }
    let (hour, minute, _) = clock_parts(value);
    let hour = if hour_changed {
        replacement
    } else if hour.is_empty() {
        "00"
    } else {
        &hour
    };
    let minute = if !hour_changed {
        replacement
    } else if minute.is_empty() {
        "00"
    } else {
        &minute
    };
    let precision = value.get(5..).unwrap_or("");
    format!("{hour}:{minute}{precision}")
}

fn change_second(value: &str, replacement: &str) -> String {
    if replacement.is_empty() {
        return String::new();
    }
    let (hour, minute, _) = clock_parts(value);
    let hour = if hour.is_empty() { "00" } else { &hour };
    let minute = if minute.is_empty() { "00" } else { &minute };
    let fraction = value.get(8..).unwrap_or("");
    format!("{hour}:{minute}:{replacement}{fraction}")
}

#[cfg(test)]
#[path = "../tests/components/date_time.rs"]
mod tests;
