use serde_json::{Value, json};

pub(super) fn handle(input: &Value) -> Option<Value> {
    let nl = input["language"] == "nl";
    let action = input["action"].as_str()?;
    let previous = &input["state"];
    if action == "open" || action == "setup" {
        return Some(if action == "open" && previous["config"].is_object() {
            page(previous.clone(), Value::Null, nl, "", false)
        } else {
            setup(previous, nl, "")
        });
    }
    if action == "disconnect" {
        let mut output = setup(&Value::Null, nl, "");
        output["save"] = json!(true);
        return Some(output);
    }

    let mut state = previous.clone();
    if input["step"].is_null() {
        if action == "connect" {
            state = json!({"config": input["fields"], "page": 1});
            let config = &state["config"];
            if ["region", "client_id", "secret", "uid"]
                .iter()
                .any(|key| super::identifier(&config[key]).is_none())
            {
                return Some(setup(
                    &state,
                    nl,
                    if nl {
                        "Vul alle vier velden in."
                    } else {
                        "Enter all four fields."
                    },
                ));
            }
        } else if action == "next" || action == "previous" {
            let page = state["page"].as_u64().unwrap_or(1);
            state["page"] = json!(if action == "next" {
                (page + 1).min(100)
            } else {
                page.saturating_sub(1).max(1)
            });
            state["offset"] = json!(0);
        } else if action == "more" || action == "fewer" {
            let offset = state["offset"].as_u64().unwrap_or(0);
            state["offset"] = json!(if action == "more" {
                offset + 10
            } else {
                offset.saturating_sub(10)
            });
        } else if let Some(index) = action.strip_prefix("switch-") {
            state["target"] = state["switches"].get(index.parse::<usize>().ok()?)?.clone();
        } else if action != "refresh" {
            return None;
        }
        return request(input, state.clone(), "token").or_else(|| Some(failed(&state, nl, action)));
    }

    let response = &input["response"]["data"];
    match state["pending"].as_str()? {
        "token" => {
            let Some(token) =
                super::handle(&json!({"operation":"token_result","response":response}))
            else {
                return Some(failed(&state, nl, action));
            };
            state["token"] = token["access_token"].clone();
            let operation = if state["target"].is_object() {
                "command"
            } else {
                "list"
            };
            request(input, state, operation)
        }
        "command" => {
            if !super::handle(&json!({"operation":"command_result","response":response}))
                .is_some_and(|result| result["accepted"] == true)
            {
                return Some(failed(&state, nl, action));
            }
            request(input, state, "status")
        }
        "status" => {
            let verified = super::handle(&json!({"operation":"status_result","response":response,"code":state["target"]["code"]}))
                .is_some_and(|result| result["on"] == !state["target"]["on"].as_bool().unwrap_or(false));
            if !verified {
                let retries = state["retries"].as_u64().unwrap_or(0);
                if retries < 2 && input["response"]["error"].is_null() {
                    state["retries"] = json!(retries + 1);
                    return request(input, state, "status");
                }
                return Some(failed(&state, nl, action));
            }
            state["confirmed"] = json!(true);
            request(input, state, "list")
        }
        "list" if response["success"] == true && response["result"].is_array() => Some(page(
            state,
            response["result"].clone(),
            nl,
            "",
            action == "connect",
        )),
        _ => Some(failed(&state, nl, action)),
    }
}

fn setup(state: &Value, nl: bool, message: &str) -> Value {
    let config = &state["config"];
    json!({"screen": {
        "message": if !message.is_empty() { message } else if nl {
            "Koppel Smart Life in Tuya Cloud. Regio: eu, we, us, ue, cn of in."
        } else { "Link Smart Life in Tuya Cloud. Region: eu, we, us, ue, cn or in." },
        "fields": [
            {"id":"region","label":if nl { "Regio" } else { "Region" },"value":config["region"].as_str().unwrap_or("eu")},
            {"id":"client_id","label":"Access ID","value":config["client_id"].as_str().unwrap_or("")},
            {"id":"secret","label":"Access Secret","value":config["secret"].as_str().unwrap_or(""),"secret":true},
            {"id":"uid","label":"Smart Life UID","value":config["uid"].as_str().unwrap_or("")}
        ],
        "buttons":[{"id":"connect","label":if nl { "Verbinden en opslaan" } else { "Connect and save" }}]
    }})
}

fn failed(state: &Value, nl: bool, action: &str) -> Value {
    let message = if nl {
        "Tuya kon het verzoek niet bevestigen. Controleer de verbinding en toegang."
    } else {
        "Tuya could not confirm the request. Check your connection and permissions."
    };
    if action == "connect" {
        setup(state, nl, message)
    } else {
        page(state.clone(), Value::Null, nl, message, false)
    }
}

fn request(input: &Value, mut state: Value, operation: &str) -> Option<Value> {
    let config = &state["config"];
    let target = &state["target"];
    let mut plan = super::plan(
        &json!({
            "region":config["region"],"client_id":config["client_id"],"secret":config["secret"],
            "uid":config["uid"],"access_token":state["token"],"timestamp_ms":input["timestamp_ms"],
            "id":target["id"],"code":target["code"],"on":!target["on"].as_bool().unwrap_or(false),
            "page":state["page"],
        }),
        operation,
    )?;
    if plan["method"] == "GET" {
        plan.as_object_mut()?.remove("body");
    }
    state["pending"] = json!(operation);
    // The next stage only needs the chosen switch and client settings.
    state.as_object_mut()?.remove("switches");
    Some(json!({"request":plan,"state":state}))
}

fn page(state: Value, devices: Value, nl: bool, message: &str, save: bool) -> Value {
    let config = &state["config"];
    let page = state["page"].as_u64().unwrap_or(1);
    let offset = state["offset"].as_u64().unwrap_or(0) as usize;
    let mut switches = Vec::new();
    let mut buttons = vec![
        json!({"id":"refresh","label":if nl { "Vernieuwen" } else { "Refresh" }}),
        json!({"id":"setup","label":if nl { "Verbinding instellen" } else { "Connection setup" }}),
        json!({"id":"disconnect","label":if nl { "Verbinding wissen" } else { "Remove connection" }}),
    ];
    if page > 1 {
        buttons.push(json!({"id":"previous","label":if nl { "Vorige apparaten" } else { "Previous devices" }}));
    }
    if devices.as_array().is_some_and(|rows| rows.len() >= 20) && page < 100 {
        buttons.push(
            json!({"id":"next","label":if nl { "Volgende apparaten" } else { "Next devices" }}),
        );
    }
    let mut offline = 0;
    let mut available = 0;
    for row in devices.as_array().into_iter().flatten() {
        let Some(device) = super::handle(&json!({"operation":"device_result","device":row})) else {
            continue;
        };
        for switch in device["switches"].as_array().into_iter().flatten() {
            if device["online"] != true {
                offline += 1;
                continue;
            }
            available += 1;
            if available <= offset || switches.len() >= 10 {
                continue;
            }
            let name = device["name"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(18)
                .collect::<String>();
            let code = switch["code"].as_str().unwrap_or("");
            let on = if state["confirmed"] == true
                && state["target"]["id"] == device["id"]
                && state["target"]["code"] == code
            {
                state["target"]["on"] == false
            } else {
                switch["on"] == true
            };
            let status = match (nl, on) {
                (true, true) => "Aan → Uit",
                (true, false) => "Uit → Aan",
                (false, true) => "On → Off",
                (false, false) => "Off → On",
            };
            buttons.push(json!({"id":format!("switch-{}",switches.len()),"label":format!("{name} {code} · {status}")}));
            switches.push(json!({"id":device["id"],"code":code,"on":on}));
        }
    }
    if offset > 0 {
        buttons.push(json!({"id":"fewer","label":if nl { "Vorige schakelaars" } else { "Previous switches" }}));
    }
    if available > offset + 10 {
        buttons.push(
            json!({"id":"more","label":if nl { "Meer schakelaars" } else { "More switches" }}),
        );
    }
    let count = switches.len();
    let summary = if !message.is_empty() {
        message.to_owned()
    } else if nl {
        format!("{count} schakelaars beschikbaar · {offline} offline · apparaatpagina {page}")
    } else {
        format!("{count} switches available · {offline} offline · device page {page}")
    };
    json!({"screen":{"message":summary,"buttons":buttons},
        "state":{"config":config,"page":page,"offset":offset,"switches":switches},
        "save":save})
}
