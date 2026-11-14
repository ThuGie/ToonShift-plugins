use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
mod screen;

thread_local! {
    static INPUT: RefCell<[u8; 32770]> = const { RefCell::new([0; 32770]) };
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

#[unsafe(no_mangle)]
pub extern "C" fn input_buffer() -> *mut u8 {
    INPUT.with(|input| input.borrow_mut().as_mut_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn render(len: usize) -> u64 {
    let result = INPUT.with(|input| {
        let input = input.borrow();
        let value: Value = serde_json::from_slice(input.get(..len)?).ok()?;
        if value["version"] == 1 {
            screen::handle(&value)
        } else {
            handle(&value)
        }
    });
    OUTPUT.with(|output| {
        let mut output = output.borrow_mut();
        *output = result
            .and_then(|result| serde_json::to_vec(&result).ok())
            .unwrap_or_default();
        ((output.as_ptr() as u64) << 32) | output.len() as u64
    })
}

fn identifier(value: &Value) -> Option<&str> {
    value.as_str().filter(|value| {
        (1..=128).contains(&value.len())
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
    })
}

fn switch_code(code: &str) -> bool {
    code == "switch"
        || code.strip_prefix("switch_").is_some_and(|suffix| {
            !suffix.is_empty()
                && suffix.len() <= 2
                && suffix.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn result(input: &Value) -> Option<&Value> {
    input["response"]["success"]
        .as_bool()?
        .then(|| &input["response"]["result"])
}

fn handle(input: &Value) -> Option<Value> {
    match input["operation"].as_str()? {
        "token_result" => {
            let token = result(input)?;
            Some(json!({
                "access_token":identifier(&token["access_token"])?,
                "expires_in":token["expire_time"].as_u64()?,
            }))
        }
        "device_result" => {
            let device = &input["device"];
            let id = identifier(&device["id"])?;
            let name: String = device["name"]
                .as_str()
                .unwrap_or(id)
                .chars()
                .filter(|c| !c.is_control())
                .take(80)
                .collect();
            let switches: Vec<_> = device["status"]
                .as_array()?
                .iter()
                .filter_map(|status| {
                    let code = status["code"].as_str()?;
                    if !switch_code(code) {
                        return None;
                    }
                    Some(json!({"code":code,"on":status["value"].as_bool()?}))
                })
                .take(16)
                .collect();
            Some(
                json!({"id":id,"name":name,"online":device["online"].as_bool(),"switches":switches}),
            )
        }
        "status_result" => {
            let code = input["code"].as_str()?;
            if !switch_code(code) {
                return None;
            }
            let state = result(input)?
                .as_array()?
                .iter()
                .find(|state| state["code"].as_str() == Some(code))?;
            Some(json!({"on":state["value"].as_bool()?}))
        }
        "command_result" => Some(json!({"accepted":result(input)?.as_bool()?})),
        operation => plan(input, operation),
    }
}

fn plan(input: &Value, operation: &str) -> Option<Value> {
    let host = match input["region"].as_str()? {
        "eu" => "openapi.tuyaeu.com",
        "we" => "openapi-weaz.tuyaeu.com",
        "us" => "openapi.tuyaus.com",
        "ue" => "openapi-ueaz.tuyaus.com",
        "cn" => "openapi.tuyacn.com",
        "in" => "openapi.tuyain.com",
        _ => return None,
    };
    let client = identifier(&input["client_id"])?;
    let secret = identifier(&input["secret"])?;
    let timestamp = input["timestamp_ms"].as_u64()?;
    let token = if operation == "token" {
        ""
    } else {
        identifier(&input["access_token"])?
    };
    let (method, path, body) = match operation {
        "token" => ("GET", "/v1.0/token?grant_type=1".into(), String::new()),
        "list" => {
            let uid = identifier(&input["uid"])?;
            let page = input["page"]
                .as_u64()
                .filter(|p| (1..=100).contains(p))
                .unwrap_or(1);
            (
                "GET",
                format!("/v1.0/users/{uid}/devices?page_no={page}&page_size=20"),
                String::new(),
            )
        }
        "status" => {
            let id = identifier(&input["id"])?;
            ("GET", format!("/v1.0/devices/{id}/status"), String::new())
        }
        "command" => {
            let id = identifier(&input["id"])?;
            let code = input["code"].as_str()?;
            if !switch_code(code) {
                return None;
            }
            let body =
                json!({"commands":[{"code":code,"value":input["on"].as_bool()?}]}).to_string();
            ("POST", format!("/v1.0/devices/{id}/commands"), body)
        }
        _ => return None,
    };
    let digest = Sha256::digest(body.as_bytes());
    let message = format!("{client}{token}{timestamp}{method}\n{digest:x}\n\n{path}");
    let signature = hmac_sha256(secret.as_bytes(), message.as_bytes());
    let mut headers = json!({
        "client_id":client, "t":timestamp.to_string(), "sign":format!("{signature:X}"),
        "sign_method":"HMAC-SHA256",
    });
    if !token.is_empty() {
        headers["access_token"] = json!(token);
    }
    Some(json!({"method":method,"host":host,"path":path,"body":body,"headers":headers}))
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> sha2::digest::Output<Sha256> {
    let digest = Sha256::digest(key);
    let key = if key.len() > 64 {
        digest.as_slice()
    } else {
        key
    };
    let mut inner = [0x36; 64];
    let mut outer = [0x5c; 64];
    for (index, byte) in key.iter().enumerate() {
        inner[index] ^= byte;
        outer[index] ^= byte;
    }
    let inner = Sha256::new()
        .chain_update(inner)
        .chain_update(message)
        .finalize();
    Sha256::new()
        .chain_update(outer)
        .chain_update(inner)
        .finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_the_body_and_query_and_rejects_non_switch_commands() {
        let mut input = json!({
            "operation":"token","region":"eu","client_id":"test_id","secret":"test_secret",
            "timestamp_ms":1588925778000_u64
        });
        let plan = handle(&input).unwrap();
        assert_eq!(plan["host"], "openapi.tuyaeu.com");
        assert_eq!(plan["path"], "/v1.0/token?grant_type=1");
        assert!(plan["headers"].get("access_token").is_none());
        assert_eq!(
            plan["headers"]["sign"],
            "756297560D595B10F95E5B81579E62D32B5B1C6AA439D25F227E430122383C6D"
        );
        input["operation"] = json!("command");
        input["access_token"] = json!("token");
        input["id"] = json!("plug");
        input["code"] = json!("switch_1");
        input["on"] = json!(false);
        let command = handle(&input).unwrap();
        assert_eq!(command["method"], "POST");
        assert_eq!(
            command["body"],
            r#"{"commands":[{"code":"switch_1","value":false}]}"#
        );
        assert_eq!(
            command["headers"]["sign"],
            "9507ABB46E9A74C6D24ED0F5572D9AF475844F6120FDF300B7F4F9CCED033898"
        );
        input["code"] = json!("unlock");
        assert!(handle(&input).is_none());
    }

    #[test]
    fn normalizes_online_switch_states_without_leaking_other_properties() {
        let output = handle(&json!({"operation":"device_result","device":{
            "id":"plug123","name":"Desk","online":false,"local_key":"secret",
            "status":[{"code":"switch_1","value":false},{"code":"unlock","value":true},{"code":"cur_power","value":24}]
        }})).unwrap();
        assert_eq!(
            output,
            json!({"id":"plug123","name":"Desk","online":false,"switches":[{"code":"switch_1","on":false}]})
        );
    }
}
