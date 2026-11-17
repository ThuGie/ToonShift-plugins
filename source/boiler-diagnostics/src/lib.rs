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

fn screen(event: &Value) -> Option<Value> {
    let action = event["action"].as_str()?;
    if event["version"] != 1 || !matches!(action, "open" | "refresh" | "telemetry") {
        return None;
    }

    let tr = |en, nl| if event["language"] == "nl" { nl } else { en };
    let boiler = &event["telemetry"]["boiler_diagnostics"];
    let unknown = tr("Unknown", "Onbekend");
    let connection = |value: &Value| match value.as_bool() {
        Some(true) => tr("Connected", "Verbonden"),
        Some(false) => tr("Offline", "Offline"),
        None => unknown,
    };
    let field = |id, label, value: String| {
        json!({
            "id": id, "label": label, "value": format!("{label} · {value}"), "readonly": true
        })
    };

    let open_therm = boiler["protocol"] == "opentherm";
    let protocol = match boiler["protocol"].as_str() {
        Some("opentherm") => "OpenTherm",
        Some("relay") => tr("On/off relay", "Aan/uit-relais"),
        _ => unknown,
    };
    let request = match boiler["heat_requested"].as_bool() {
        Some(true) => tr("Heating requested", "Warmtevraag actief"),
        Some(false) => tr("Idle", "Geen warmtevraag"),
        None => unknown,
    };
    let mut fields = vec![
        field("protocol", "Protocol", protocol.into()),
        field(
            "module",
            tr("Boiler adapter", "Ketelmodule"),
            connection(&boiler["module_connected"]).into(),
        ),
        field(
            "request",
            tr("Toon heat request", "Warmtevraag Toon"),
            request.into(),
        ),
    ];
    let limits = if action == "telemetry" {
        &event["state"]["limits"]
    } else {
        &boiler["limits"]
    };
    if open_therm {
        let burner = match boiler["burner"].as_u64() {
            Some(0) => tr("Idle", "Uit"),
            Some(1) => tr("Space heating", "Verwarming"),
            Some(2) => tr("Hot water", "Warm water"),
            Some(3) => tr("Preheating", "Voorverwarmen"),
            _ => unknown,
        };
        let measurement = |value: &Value, unit| {
            value
                .as_u64()
                .map_or_else(|| unknown.to_owned(), |number| format!("{number} {unit}"))
        };
        fields.extend([
            field(
                "ot_link",
                tr("OpenTherm link", "OpenTherm-verbinding"),
                connection(&boiler["opentherm_connected"]).into(),
            ),
            field("burner", tr("Burner", "Brander"), burner.into()),
            field(
                "flow",
                tr("Requested flow", "Gevraagde aanvoer"),
                measurement(&boiler["requested_flow_celsius"], "°C"),
            ),
            field(
                "modulation",
                tr("Reported modulation", "Gemelde modulatie"),
                measurement(&boiler["reported_modulation_percent"], "%"),
            ),
        ]);
    }
    if let Some(max) = limits["max_heater_celsius"].as_f64() {
        fields.push(field(
            "limit",
            tr("Configured heater limit", "Ingestelde ketellimiet"),
            format!("{max} °C"),
        ));
    }
    let message = if !boiler.is_object() {
        tr(
            "Update ToonShift to view boiler diagnostics.",
            "Werk ToonShift bij voor keteldiagnose.",
        )
    } else if boiler["protocol"] == "relay" {
        tr(
            "Relay mode reports the Toon heat request.",
            "Aan/uit toont de warmtevraag van Toon.",
        )
    } else {
        tr(
            "Live data from the Toon heating service.",
            "Actuele gegevens van de verwarmingsdienst.",
        )
    };
    Some(json!({
        "screen": {
            "message": message,
            "fields": fields,
            "buttons": [{"id":"refresh","label":tr("Refresh","Vernieuwen")}]
        },
        "state": {"limits": limits}
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_shows_adapter_and_request_without_burner() {
        let result = screen(
            &json!({"version":1,"action":"open","telemetry":{"boiler_diagnostics":{
                "protocol":"relay","module_connected":true,"heat_requested":true
            }}}),
        )
        .unwrap();
        let fields = result["screen"]["fields"].as_array().unwrap();
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[1]["value"], "Boiler adapter · Connected");
        assert_eq!(fields[2]["value"], "Toon heat request · Heating requested");
    }

    #[test]
    fn opentherm_keeps_configured_limit_on_periodic_update() {
        let result = screen(&json!({"version":1,"action":"telemetry","language":"nl",
            "telemetry":{"boiler_diagnostics":{"protocol":"opentherm","opentherm_connected":false}},
            "state":{"limits":{"max_heater_celsius":75.0}}
        }))
        .unwrap();
        let fields = result["screen"]["fields"].as_array().unwrap();
        assert_eq!(fields[3]["value"], "OpenTherm-verbinding · Offline");
        assert_eq!(fields[4]["value"], "Brander · Onbekend");
        assert_eq!(fields[7]["value"], "Ingestelde ketellimiet · 75 °C");
    }
}
