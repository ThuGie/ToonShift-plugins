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

fn measurements(data: &Value, nl: bool) -> Vec<Value> {
    let mut rows = Vec::new();
    let tr = |en, dutch| if nl { dutch } else { en };
    let mut add = |id, label, number: Option<f64>, unit, decimals| {
        if let Some(number) = number.filter(|value| value.is_finite()) {
            let value = format!("{number:.decimals$}");
            let value = if nl { value.replace('.', ",") } else { value };
            rows.push(json!({"id":id,"label":label,"value":format!("{label} · {value} {unit}"),"readonly":true}));
        }
    };
    let counter = |direction: &str| {
        data[format!("total_power_{direction}_kwh")]
            .as_f64()
            .or_else(|| {
                let tariff1 = data[format!("total_power_{direction}_t1_kwh")].as_f64()?;
                Some(tariff1 + data[format!("total_power_{direction}_t2_kwh")].as_f64()?)
            })
    };
    let external = |kind: &str| {
        data["external"]
            .as_array()?
            .iter()
            .find(|row| row["type"] == kind && row["unit"] == "m3")?["value"]
            .as_f64()
    };
    if let Some(power) = data["active_power_w"].as_f64() {
        let label = if power < 0.0 {
            tr("Export now", "Teruglevering nu")
        } else {
            tr("Import now", "Afname nu")
        };
        add("power", label, Some(power.abs()), "W", 0);
    }
    add(
        "import",
        tr("Total import", "Afnamestand"),
        counter("import"),
        "kWh",
        3,
    );
    add(
        "export",
        tr("Total export", "Terugleverstand"),
        counter("export"),
        "kWh",
        3,
    );
    add(
        "gas",
        tr("Gas meter", "Gasstand"),
        external("gas_meter").or_else(|| data["total_gas_m3"].as_f64()),
        "m³",
        3,
    );
    add(
        "water",
        tr("Water meter", "Waterstand"),
        data["total_liter_m3"]
            .as_f64()
            .or_else(|| external("water_meter")),
        "m³",
        3,
    );
    add(
        "water-flow",
        tr("Water flow", "Waterverbruik nu"),
        data["active_liter_lpm"].as_f64(),
        "L/min",
        1,
    );
    rows
}

fn screen(event: &Value) -> Option<Value> {
    if event["version"] != 1 {
        return None;
    }
    let nl = event["language"] == "nl";
    let tr = |en, dutch| if nl { dutch } else { en };
    let action = event["action"].as_str()?;
    let initial = event["state"]["address"].as_str().unwrap_or("");
    let address = if action == "open" {
        initial
    } else {
        event["fields"]["lan_address"].as_str()?
    };
    let mut rows = vec![
        json!({"id":"lan_address","label":tr("Meter IPv4 address","IPv4-adres meter"),"value":address}),
    ];
    let mut save = false;
    let message = match action {
        "open" => tr(
            "Enable the local API in HomeWizard. Enter the meter IP.",
            "Schakel de lokale API in HomeWizard in. Voer het meter-IP in.",
        ),
        "read" if event["step"].is_null() => {
            return Some(json!({"request":{"host":"user","path":"/api/v1/data"}}));
        }
        "read" => {
            let readings = measurements(&event["response"]["data"], nl);
            if readings.is_empty() {
                tr(
                    "No readings. Check the meter IP and local API v1.",
                    "Geen meetwaarden. Controleer het meter-IP en lokale API v1.",
                )
            } else {
                save = true;
                rows.extend(readings);
                tr("Latest local meter readings", "Laatste lokale meterstanden")
            }
        }
        _ => return None,
    };
    Some(json!({"screen":{"message":message,"fields":rows,
        "buttons":[{"id":"read","label":tr("Read meter","Lees meter")}]},
        "state":{"address":address},"save":save}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_p1_export_and_separate_gas_and_water() {
        let rows = measurements(
            &json!({
                "active_power_w": -540,
                "total_power_import_t1_kwh":1.2, "total_power_import_t2_kwh":2.9,
                "total_power_export_kwh":0, "total_gas_m3":5.0,
                "external":[{"type":"water_meter","unit":"m3","value":7.2}]
            }),
            false,
        );
        assert_eq!(rows[0]["value"], "Export now · 540 W");
        assert_eq!(rows[1]["value"], "Total import · 4.100 kWh");
        assert_eq!(rows[2]["value"], "Total export · 0.000 kWh");
        assert_eq!(rows[4]["value"], "Water meter · 7.200 m³");
    }

    #[test]
    fn water_only_meter_preserves_zero_and_skips_absent_fields() {
        let rows = measurements(&json!({"total_liter_m3":4.2,"active_liter_lpm":0}), true);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1]["value"], "Waterverbruik nu · 0,0 L/min");
    }
}
