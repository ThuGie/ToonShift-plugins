use serde_json::{Value, json};
use std::cell::RefCell;

thread_local! {
    static INPUT: RefCell<[u8; 32770]> = const { RefCell::new([0; 32770]) };
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

#[unsafe(no_mangle)]
pub extern "C" fn input_buffer() -> *mut u8 {
    INPUT.with(|input| input.borrow_mut().as_mut_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn render(length: usize) -> u64 {
    let screen = INPUT.with(|input| {
        let input = input.borrow();
        let event = serde_json::from_slice(input.get(..length)?).ok()?;
        screen(&event)
    });
    OUTPUT.with(|output| {
        let mut output = output.borrow_mut();
        *output = screen
            .map(|screen| screen.to_string().into_bytes())
            .unwrap_or_default();
        ((output.as_ptr() as u64) << 32) | output.len() as u64
    })
}

fn reading(sample: &Value, unit: &str, range: std::ops::RangeInclusive<f64>) -> Option<(f64, u64)> {
    let age = sample["age_seconds"].as_u64()?;
    let value = sample["value"].as_f64()?;
    (sample["unit"] == unit && age <= 300 && range.contains(&value)).then_some((value, age))
}

fn dew_point(temperature: f64, humidity: f64) -> f64 {
    let vapor = (humidity / 100.0).ln() + 17.625 * temperature / (243.04 + temperature);
    243.04 * vapor / (17.625 - vapor)
}

fn screen(event: &Value) -> Option<Value> {
    if event["version"] != 1
        || !matches!(
            event["action"].as_str(),
            Some("open" | "refresh" | "telemetry")
        )
    {
        return None;
    }
    let nl = event["language"] == "nl";
    let tr = |en, dutch| if nl { dutch } else { en };
    let snapshot = &event["telemetry"];
    let temp = reading(&snapshot["room_temperature"], "°C", -20.0..=60.0);
    let humidity = reading(&snapshot["room_humidity"], "%", 0.0..=100.0);
    let dew = temp
        .zip(humidity)
        .and_then(|((temperature, t_age), (humidity, h_age))| {
            (humidity >= 1.0 && t_age.abs_diff(h_age) <= 120)
                .then(|| dew_point(temperature, humidity))
        });
    let message = match humidity {
        None => tr(
            "Waiting for a fresh humidity reading.",
            "Wachten op een recente vochtmeting.",
        ),
        Some((rh, _)) if rh < 30.0 => tr("Low indoor humidity.", "Lage luchtvochtigheid."),
        Some((rh, _)) if rh <= 50.0 => tr(
            "Humidity is in the 30–50% range.",
            "Luchtvochtigheid ligt tussen 30 en 50%.",
        ),
        Some((rh, _)) if rh < 60.0 => tr(
            "Humidity is above 50%.",
            "Luchtvochtigheid is hoger dan 50%.",
        ),
        Some(_) => tr(
            "High humidity. Check cold surfaces for condensation.",
            "Hoge luchtvochtigheid. Controleer koude oppervlakken op condens.",
        ),
    };
    let missing = tr("Unavailable", "Niet beschikbaar");
    let decimal = |value: f64| {
        let text = format!("{value:.1} °C");
        if nl { text.replace('.', ",") } else { text }
    };
    Some(json!({"screen":{
        "message":message,
        "fields":[
            {"id":"temperature","label":tr("Room temperature","Kamertemperatuur"),
             "value":temp.map(|(value, _)| decimal(value)).unwrap_or_else(|| missing.into()),"readonly":true},
            {"id":"humidity","label":tr("Relative humidity","Relatieve luchtvochtigheid"),
             "value":humidity.map(|(value, _)| format!("{value:.0}%")).unwrap_or_else(|| missing.into()),"readonly":true},
            {"id":"dew_point","label":tr("Estimated dew point","Geschat dauwpunt"),
             "value":dew.map(decimal).unwrap_or_else(|| missing.into()),"readonly":true},
        ],
        "buttons":[{"id":"refresh","label":tr("Refresh","Vernieuwen")}]
    }}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(temperature: f64, humidity: f64, temperature_age: u64, humidity_age: u64) -> Value {
        json!({"version":1,"action":"telemetry","telemetry":{
            "room_temperature":{"value":temperature,"unit":"°C","age_seconds":temperature_age},
            "room_humidity":{"value":humidity,"unit":"%","age_seconds":humidity_age}
        }})
    }

    #[test]
    fn estimates_dew_point_and_formats_dutch_output() {
        let mut event = fixture(20.0, 50.0, 14, 15);
        event["language"] = json!("nl");
        let screen = screen(&event).unwrap();
        assert_eq!(screen["screen"]["fields"][2]["value"], "9,3 °C");
        assert_eq!(screen["screen"]["fields"][1]["value"], "50%");
    }

    #[test]
    fn gates_dew_point_on_fresh_contemporaneous_samples() {
        for event in [
            fixture(20.0, 80.0, 2, 301),
            fixture(20.0, 50.0, 2, 125),
            fixture(20.0, 0.0, 2, 2),
        ] {
            assert_eq!(
                screen(&event).unwrap()["screen"]["fields"][2]["value"],
                "Unavailable"
            );
        }
    }

    #[test]
    fn high_humidity_and_saturation_remain_bounded() {
        let output = screen(&fixture(21.0, 100.0, 1, 2)).unwrap();
        assert_eq!(output["screen"]["fields"][2]["value"], "21.0 °C");
        assert!(
            output["screen"]["message"]
                .as_str()
                .unwrap()
                .starts_with("High humidity")
        );
    }
}
