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

fn number(fields: &Value, id: &str, maximum: f64) -> Option<f64> {
    let number: f64 = fields[id].as_str()?.trim().replace(',', ".").parse().ok()?;
    (0.0..=maximum).contains(&number).then_some(number)
}

fn estimate(fields: &Value) -> Option<(f64, f64)> {
    let kwh = number(fields, "watts", 100_000.0)? * number(fields, "hours", 24.0)? * 365.0 / 1000.0;
    Some((kwh, kwh * number(fields, "rate", 100.0)?))
}

fn screen(event: &Value) -> Option<Value> {
    if event["version"] != 1 {
        return None;
    }
    let nl = event["language"] == "nl";
    let action = event["action"].as_str()?;
    let defaults = json!({"watts":"5","hours":"24","rate":"0.30"});
    let fields = match action {
        "open" if event["state"].is_object() => &event["state"],
        "open" | "reset" => &defaults,
        "calculate" => &event["fields"],
        _ => return None,
    };
    let result = estimate(fields);
    let message = match result {
        Some((kwh, cost)) if nl => {
            format!("{kwh:.1} kWh per jaar · geschat €{cost:.2}").replace('.', ",")
        }
        Some((kwh, cost)) => format!("{kwh:.1} kWh per year · estimated €{cost:.2}"),
        None if nl => "Controleer vermogen, uren per dag (0–24) en tarief.".into(),
        None => "Check power, hours per day (0–24), and unit price.".into(),
    };
    let field = |id: &str, en, dutch| {
        let value: String = fields[id]
            .as_str()
            .unwrap_or("")
            .chars()
            .filter(|character| !character.is_control())
            .take(32)
            .collect();
        json!({"id":id,"label":if nl { dutch } else { en },"value":value})
    };
    Some(json!({
        "screen":{
            "message":message,
            "fields":[
                field("watts", "Standby power (W)", "Stand-byvermogen (W)"),
                field("hours", "Hours per day", "Uren per dag"),
                field("rate", "Electricity rate (€/kWh)", "Stroomtarief (€/kWh)")
            ],
            "buttons":[
                {"id":"calculate","label":if nl { "Bereken en bewaar" } else { "Calculate and save" }},
                {"id":"reset","label":if nl { "Standaardwaarden" } else { "Restore defaults" }}
            ]
        },
        "state":fields,
        "save":action != "open" && result.is_some()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_annual_use_and_cost_with_dutch_decimal_input() {
        let fields = json!({"watts":"10","hours":"24","rate":"0,25"});
        let (energy, cost) = estimate(&fields).unwrap();
        assert!((energy - 87.6).abs() < 1e-10);
        assert!((cost - 21.9).abs() < 1e-10);
        let output =
            screen(&json!({"version":1,"action":"calculate","language":"nl","fields":fields}))
                .unwrap();
        assert_eq!(output["save"], true);
        assert!(
            output["screen"]["message"]
                .as_str()
                .unwrap()
                .contains("€21,90")
        );
    }

    #[test]
    fn rejects_invalid_values_without_saving_and_preserves_zero() {
        let output = screen(&json!({"version":1,"action":"calculate","fields":{"watts":"1","hours":"25","rate":"NaN"}})).unwrap();
        assert_eq!(output["save"], false);
        assert_eq!(
            estimate(&json!({"watts":"0","hours":"24","rate":"0.30"})),
            Some((0.0, 0.0))
        );
    }
}
