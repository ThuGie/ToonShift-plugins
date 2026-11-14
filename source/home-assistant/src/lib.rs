use serde_json::{Value, json};
use std::cell::RefCell;

thread_local! {
    static INPUT: RefCell<[u8; 512]> = const { RefCell::new([0; 512]) };
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

#[unsafe(no_mangle)]
pub extern "C" fn input_buffer() -> *mut u8 {
    INPUT.with(|input| input.borrow_mut().as_mut_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn render(length: usize) -> u64 {
    let plan = INPUT.with(|input| {
        let input = input.borrow();
        let value = serde_json::from_slice(input.get(..length)?).ok()?;
        request(&value)
    });
    OUTPUT.with(|output| {
        let mut output = output.borrow_mut();
        *output = plan
            .and_then(|plan| serde_json::to_vec(&plan).ok())
            .unwrap_or_default();
        ((output.as_ptr() as u64) << 32) | output.len() as u64
    })
}

fn request(input: &Value) -> Option<Value> {
    match input["operation"].as_str()? {
        "list" => Some(json!({"path":"/api/states"})),
        "read" => Some(json!({"path":format!("/api/states/{}", input["id"].as_str()?)})),
        "set" => {
            let target = if input["on"].as_bool()? { "on" } else { "off" };
            Some(json!({
                "path":format!("/api/services/switch/turn_{target}"),
                "body":{"entity_id": input["id"].as_str()?},
            }))
        }
        _ => None,
    }
}
