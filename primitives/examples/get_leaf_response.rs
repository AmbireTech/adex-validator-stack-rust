use primitives::sentry::GetLeafResponse;
use serde_json::{from_value, json};

fn main() {
    let json = json!({
        "merkleProof": "8ea7760ca2dbbe00673372afbf8b05048717ce8a305f1f853afac8c244182e0c",
    });

    assert!(from_value::<GetLeafResponse>(json).is_ok());
}
