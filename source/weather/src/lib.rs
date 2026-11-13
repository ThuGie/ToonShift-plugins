use serde_json::{Value, json};
use std::cell::RefCell;

thread_local! {
    static INPUT: RefCell<[u8; 32770]> = const { RefCell::new([0; 32770]) };
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

#[unsafe(no_mangle)]
pub extern "C" fn input_buffer() -> *mut u8 {
    INPUT.with(|buffer| buffer.borrow_mut().as_mut_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn render(length: usize) -> u64 {
    let output = INPUT.with(|buffer| {
        let buffer = buffer.borrow();
        let [operation, locale, body @ ..] = buffer.get(..length)? else {
            return None;
        };
        let document: Value = serde_json::from_slice(body).ok()?;
        match operation {
            0 => forecast(&document, *locale == 1),
            1 => Some(locations(&document)),
            _ => None,
        }
    });
    OUTPUT.with(|buffer| {
        let mut buffer = buffer.borrow_mut();
        *buffer = output
            .and_then(|output| serde_json::to_vec(&output).ok())
            .unwrap_or_default();
        ((buffer.as_ptr() as u64) << 32) | buffer.len() as u64
    })
}

fn forecast(document: &Value, dutch: bool) -> Option<Value> {
    let current = &document["current"];
    let temperature = current["temperature_2m"].as_f64()?;
    let daily = &document["daily"];
    let rows: Vec<_> = (0..5)
        .filter_map(|day| {
            let date = daily["time"][day].as_str()?;
            let low = daily["temperature_2m_min"][day].as_f64()?;
            let high = daily["temperature_2m_max"][day].as_f64()?;
            Some(json!({
                "date": date,
                "range": format!("{low:.0}°C / {high:.0}°C"),
                "condition": condition(&daily["weather_code"][day], dutch),
            }))
        })
        .collect();
    Some(json!({
        "value": format!("{temperature:.0}°C"),
        "detail": condition(&current["weather_code"], dutch),
        "rows": rows,
    }))
}

fn locations(document: &Value) -> Value {
    let places = document["results"].as_array().into_iter().flatten().filter_map(|row| {
        let name = row["name"].as_str()?;
        let region = row["admin1"].as_str().or_else(|| row["country"].as_str());
        Some(json!({
            "name": region.map_or_else(|| name.to_owned(), |region| format!("{name}, {region}")),
            "latitude": row["latitude"].as_f64()?,
            "longitude": row["longitude"].as_f64()?,
        }))
    }).take(5).collect();
    Value::Array(places)
}

fn condition(code: &Value, dutch: bool) -> &'static str {
    let (en, nl) = match code.as_i64() {
        Some(0) => ("Clear", "Helder"),
        Some(1) => ("Mainly clear", "Meestal helder"),
        Some(2) => ("Partly cloudy", "Halfbewolkt"),
        Some(3) => ("Overcast", "Bewolkt"),
        Some(45) => ("Fog", "Mist"),
        Some(48) => ("Freezing fog", "Aanvriezende mist"),
        Some(51) => ("Light drizzle", "Lichte motregen"),
        Some(53) => ("Drizzle", "Motregen"),
        Some(55) => ("Heavy drizzle", "Zware motregen"),
        Some(56 | 57) => ("Freezing drizzle", "Aanvriezende motregen"),
        Some(61) => ("Light rain", "Lichte regen"),
        Some(63) => ("Rain", "Regen"),
        Some(65) => ("Heavy rain", "Zware regen"),
        Some(66 | 67) => ("Freezing rain", "IJzel"),
        Some(71) => ("Light snow", "Lichte sneeuw"),
        Some(73) => ("Snow", "Sneeuw"),
        Some(75) => ("Heavy snow", "Zware sneeuw"),
        Some(77) => ("Snow grains", "Motsneeuw"),
        Some(80) => ("Light showers", "Lichte buien"),
        Some(81) => ("Showers", "Buien"),
        Some(82) => ("Heavy showers", "Zware buien"),
        Some(85 | 86) => ("Snow showers", "Sneeuwbuien"),
        Some(95) => ("Thunderstorm", "Onweer"),
        Some(96 | 99) => ("Thunderstorm with hail", "Onweer met hagel"),
        _ => ("", ""),
    };
    if dutch { nl } else { en }
}
