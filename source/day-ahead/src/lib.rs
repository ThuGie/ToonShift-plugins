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

const HOST: &str = "api.energy-charts.info";

fn quotes(data: &Value) -> Option<Vec<(u64, Option<f64>)>> {
    if data["unit"] != "EUR / MWh" {
        return None;
    }
    let times = data["unix_seconds"].as_array()?;
    let prices = data["price"].as_array()?;
    if times.is_empty() || times.len() != prices.len() || times.len() > 120 {
        return None;
    }
    let rows: Vec<_> = times
        .iter()
        .zip(prices)
        .map(|(time, price)| {
            Some((
                time.as_u64()?,
                price.as_f64().filter(|price| price.is_finite()),
            ))
        })
        .collect::<Option<_>>()?;
    rows.windows(2)
        .all(|pair| pair[0].0 < pair[1].0)
        .then_some(rows)
}

fn screen(event: &Value) -> Option<Value> {
    if event["version"] != 1 {
        return None;
    }
    let nl = event["language"] == "nl";
    let tr = |en, dutch| if nl { dutch } else { en };
    let now = event["timestamp_ms"].as_u64()? / 1000;
    let mut rows = Vec::new();
    let message = match event["action"].as_str()? {
        "open" => tr(
            "Refresh for Dutch day-ahead spot prices.",
            "Vernieuw voor Nederlandse beursprijzen.",
        ),
        "refresh" if event["step"].is_null() => {
            return Some(json!({"request":{
                "host":HOST, "path":format!("/price?bzn=NL&start={}&end={}", now.saturating_sub(3600), now.saturating_add(86400))
            }}));
        }
        "refresh" => {
            if let Some(prices) = quotes(&event["response"]["data"]) {
                let format_price = |price: f64| {
                    let text = format!("{:.2} ct/kWh", price / 10.0);
                    if nl { text.replace('.', ",") } else { text }
                };
                let interval = prices
                    .windows(2)
                    .filter_map(|pair| {
                        let seconds = pair[1].0 - pair[0].0;
                        matches!(seconds, 900 | 1800 | 3600).then_some(seconds)
                    })
                    .min()
                    .unwrap_or(900);
                if let Some(price) = prices
                    .iter()
                    .rev()
                    .find(|(time, _)| *time <= now)
                    .filter(|(time, _)| now - time < interval)
                    .and_then(|(_, price)| *price)
                {
                    rows.push(json!({"id":"current","label":tr("Current spot price","Huidige beursprijs"),"value":format!("{} · {}",tr("Current","Nu"),format_price(price)),"readonly":true}));
                }
                let mut upcoming: Vec<_> = prices
                    .iter()
                    .filter_map(|&(time, price)| {
                        (time > now && time - now <= 86400).then_some((time, price?))
                    })
                    .collect();
                upcoming.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
                for (index, (start, price)) in upcoming.into_iter().take(6).enumerate() {
                    let minutes = (start - now).div_ceil(60);
                    let wait = format!("{}h {:02}m", minutes / 60, minutes % 60);
                    rows.push(json!({"id":format!("price-{index}"),"label":tr("Upcoming price","Komend tarief"),"value":format!("{wait} · {}",format_price(price)),"readonly":true}));
                }
                if rows.is_empty() {
                    tr(
                        "No valid prices for the current or upcoming intervals.",
                        "Geen geldige prijzen voor nu of later.",
                    )
                } else {
                    tr(
                        "Until start · excl. tax and fees · Energy-Charts.info / SMARD",
                        "Tot start · excl. belasting en kosten · Energy-Charts.info / SMARD",
                    )
                }
            } else {
                tr(
                    "Prices unavailable. Try refreshing again later.",
                    "Prijzen niet beschikbaar. Probeer het later opnieuw.",
                )
            }
        }
        _ => return None,
    };
    Some(json!({"screen":{"message":message,"fields":rows,
        "buttons":[{"id":"refresh","label":tr("Refresh","Vernieuwen")}]}}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_negative_current_price_and_ignores_missing_quotes() {
        let output = screen(&json!({"version":1,"action":"refresh","step":1,"timestamp_ms":930000,
            "response":{"data":{"unit":"EUR / MWh","unix_seconds":[900,1800,2700,3600],"price":[-20,null,30,10]}}})).unwrap();
        let rows = output["screen"]["fields"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["value"], "Current · -2.00 ct/kWh");
        assert!(rows[1]["value"].as_str().unwrap().ends_with("1.00 ct/kWh"));
    }
}
