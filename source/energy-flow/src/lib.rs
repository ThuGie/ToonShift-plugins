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

fn reading(sample: &Value, unit: &str, max_age: u64) -> Option<(f64, u64)> {
    let age = sample["age_seconds"].as_u64()?;
    let value = sample["value"].as_f64()?;
    (sample["unit"] == unit && age <= max_age && value.is_finite()).then_some((value, age))
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
    let electricity = match reading(&snapshot["electricity"], "W", 120) {
        Some((watts, age)) => {
            let direction = if watts < 0.0 {
                tr("Grid export", "Teruglevering")
            } else {
                tr("Grid import", "Afname")
            };
            format!("{direction} · {:.0} W · {age}s", watts.abs())
        }
        None => tr(
            "Electricity · No recent data",
            "Stroom · Geen recente gegevens",
        )
        .into(),
    };
    let gas = match reading(&snapshot["gas"], "L/h", 7200).filter(|(liters, _)| *liters >= 0.0) {
        Some((liters, age)) => format!("Gas · {liters:.0} L/h · {}m", age / 60),
        None => tr("Gas · No recent data", "Gas · Geen recente gegevens").into(),
    };
    Some(json!({"screen":{
        "message":tr("Live meter flow · reading age shown", "Live meterwaarden · leeftijd van meting"),
        "fields":[
            {"id":"electricity","label":tr("Electricity","Stroom"),"value":electricity,"readonly":true},
            {"id":"gas","label":"Gas","value":gas,"readonly":true}
        ],
        "buttons":[{"id":"refresh","label":tr("Refresh","Vernieuwen")}]
    }}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_grid_export_and_keeps_valid_zero() {
        let output = screen(&json!({"version":1,"action":"open","telemetry":{
            "electricity":{"value":-425,"unit":"W","age_seconds":7},
            "gas":{"value":0,"unit":"L/h","age_seconds":200}
        }}))
        .unwrap();
        assert_eq!(
            output["screen"]["fields"][0]["value"],
            "Grid export · 425 W · 7s"
        );
        assert_eq!(output["screen"]["fields"][1]["value"], "Gas · 0 L/h · 3m");
    }

    #[test]
    fn stale_or_incompatible_values_are_unavailable() {
        let output = screen(&json!({"version":1,"action":"telemetry","telemetry":{
            "electricity":{"value":300,"unit":"W","age_seconds":121},
            "gas":{"value":1,"unit":"m³","age_seconds":20}
        }}))
        .unwrap();
        for field in output["screen"]["fields"].as_array().unwrap() {
            assert!(field["value"].as_str().unwrap().contains("No recent data"));
        }
    }
}
