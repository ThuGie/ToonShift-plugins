use serde_json::{Value, json};

pub(super) fn handle(input: &Value) -> Option<Value> {
    let nl = input["language"] == "nl";
    let action = input["action"].as_str()?;
    let mut state = input["state"].clone();
    let text = |en, dutch| if nl { dutch } else { en };
    match action {
        "open" => {
            return Some(if state["config"].is_object() {
                controls(&state, &Value::Null, nl, "", false)
            } else {
                setup(&state, nl, "", false)
            });
        }
        "setup" => return Some(setup(&state, nl, "", false)),
        "disconnect" => return Some(setup(&Value::Null, nl, "", true)),
        _ => {}
    }
    if input["step"].is_null() {
        if action == "connect" {
            let token = input["fields"]["token"].as_str().unwrap_or("");
            if !(20..=2048).contains(&token.len())
                || !token
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            {
                return Some(setup(
                    &state,
                    nl,
                    text(
                        "Enter a valid access token.",
                        "Voer een geldig toegangstoken in.",
                    ),
                    false,
                ));
            }
            state = json!({"config":input["fields"],"offset":0});
        } else if let Some(index) = action.strip_prefix("switch-") {
            let row = state["switches"].get(index.parse::<usize>().ok()?)?;
            let id = row[0].as_str().filter(|id| valid_id(id))?;
            let on = row[1].as_bool()?;
            let plan = super::request(&json!({"operation":"set","id":id,"on":!on}))?;
            state["target"] = json!({"id":id,"on":!on});
            return request(state, plan, "set");
        } else if action == "next" || action == "previous" {
            let offset = state["offset"].as_u64().unwrap_or(0);
            state["offset"] = json!(if action == "next" {
                (offset + 12).min(4092)
            } else {
                offset.saturating_sub(12)
            });
        } else if action != "refresh" {
            return None;
        }
        return list(state);
    }
    let response = &input["response"]["data"];
    if input["response"]["error"].is_string() {
        let message = text(
            "Home Assistant request failed. Check HTTPS and your token.",
            "Verzoek mislukt. Controleer HTTPS en het toegangstoken.",
        );
        return Some(if action == "connect" {
            setup(&state, nl, message, false)
        } else {
            controls(&state, &Value::Null, nl, message, false)
        });
    }
    match state["pending"].as_str()? {
        "set" => {
            let plan = super::request(&json!({"operation":"read","id":state["target"]["id"]}))?;
            request(state, plan, "read")
        }
        "read" => {
            let expected = if state["target"]["on"] == true {
                "on"
            } else {
                "off"
            };
            if response["entity_id"] != state["target"]["id"] || response["state"] != expected {
                let retries = state["retries"].as_u64().unwrap_or(0);
                if retries < 2 {
                    state["retries"] = json!(retries + 1);
                    let plan =
                        super::request(&json!({"operation":"read","id":state["target"]["id"]}))?;
                    return request(state, plan, "read");
                }
                return Some(controls(
                    &state,
                    &Value::Null,
                    nl,
                    text(
                        "Home Assistant did not confirm the switch state.",
                        "Home Assistant heeft de schakelstand niet bevestigd.",
                    ),
                    false,
                ));
            }
            state["confirmed"] = json!(true);
            list(state)
        }
        "list" if response.is_array() => {
            Some(controls(&state, response, nl, "", action == "connect"))
        }
        _ => None,
    }
}

fn valid_id(id: &str) -> bool {
    id.strip_prefix("switch.").is_some_and(|name| {
        (1..=96).contains(&name.len())
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    })
}

fn request(mut state: Value, mut plan: Value, pending: &str) -> Option<Value> {
    plan["host"] = json!("user");
    if let Some(body) = plan.get("body") {
        plan["body"] = json!(body.to_string());
    }
    let token = state["config"]["token"].as_str()?;
    plan["headers"] = json!({"Authorization":format!("Bearer {token}")});
    state.as_object_mut()?.remove("switches");
    state["pending"] = json!(pending);
    Some(json!({"request":plan,"state":state}))
}

fn list(state: Value) -> Option<Value> {
    let offset = state["offset"].as_u64().unwrap_or(0);
    request(
        state,
        json!({"path":"/api/states","array":{
            "prefix_pointer":"/entity_id","prefix":"switch.",
            "pointers":["/entity_id","/state","/attributes/friendly_name"],
            "limit":13,"offset":offset
        }}),
        "list",
    )
}

fn setup(state: &Value, nl: bool, message: &str, save: bool) -> Value {
    json!({"screen":{
        "message": if !message.is_empty() { message } else if nl { "Voer de HTTPS-server en het toegangstoken in." }
            else { "Enter your HTTPS server and long-lived access token." },
        "fields":[
            {"id":"https_origin","label":"Home Assistant HTTPS","value":state["config"]["https_origin"].as_str().unwrap_or("https://")},
            {"id":"token","label":if nl { "Toegangstoken" } else { "Long-lived access token" },
             "value":state["config"]["token"].as_str().unwrap_or(""),"secret":true}
        ],
        "buttons":[{"id":"connect","label":if nl { "Verbinden en opslaan" } else { "Connect and save" }}]
    },"save":save})
}

fn controls(state: &Value, rows: &Value, nl: bool, message: &str, save: bool) -> Value {
    let config = &state["config"];
    let offset = state["offset"].as_u64().unwrap_or(0);
    let text = |en, dutch| if nl { dutch } else { en };
    let mut buttons = vec![
        json!({"id":"refresh","label":text("Refresh","Vernieuwen")}),
        json!({"id":"setup","label":text("Connection setup","Verbinding instellen")}),
        json!({"id":"disconnect","label":text("Remove connection","Verbinding wissen")}),
    ];
    if offset > 0 {
        buttons
            .push(json!({"id":"previous","label":text("Previous switches","Vorige schakelaars")}));
    }
    if rows.as_array().is_some_and(|rows| rows.len() > 12) {
        buttons.push(json!({"id":"next","label":text("Next switches","Volgende schakelaars")}));
    }
    let mut switches = Vec::new();
    for row in rows.as_array().into_iter().flatten().take(12) {
        let Some(id) = row[0].as_str().filter(|id| valid_id(id)) else {
            continue;
        };
        let on = match row[1].as_str() {
            Some("on") => true,
            Some("off") => false,
            _ => continue,
        };
        let on = if state["confirmed"] == true && state["target"]["id"] == id {
            state["target"]["on"] == true
        } else {
            on
        };
        let name: String = row[2]
            .as_str()
            .unwrap_or(id)
            .chars()
            .filter(|c| !c.is_control())
            .take(22)
            .collect();
        let label = if on {
            text("On → Off", "Aan → Uit")
        } else {
            text("Off → On", "Uit → Aan")
        };
        buttons.push(
            json!({"id":format!("switch-{}", switches.len()),"label":format!("{name} · {label}")}),
        );
        switches.push(json!([id, on]));
    }
    json!({"screen":{
        "message":if !message.is_empty() { message } else { text("Refresh to read your switches.", "Vernieuw om schakelaars te lezen.") },
        "fields":[{"id":"https_origin","label":"Home Assistant HTTPS","value":config["https_origin"],"readonly":true}],
        "buttons":buttons
    },"state":{"config":config,"offset":offset,"switches":switches},"save":save})
}
