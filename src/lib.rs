#[macro_export]
#[cfg(target_arch = "wasm32")]
macro_rules! console_err {
        ($($args: tt)+) => {{
            let here = format!("{}:{}]", file!(), line!());
            let msg = format!($($args)+);
            let msg_formatted = format!("{} {}", here, msg);
            let msg_js = wasm_bindgen::JsValue::from(msg_formatted);
            web_sys::console::error_1(&msg_js);
        }};
    }

#[macro_export]
#[cfg(target_arch = "wasm32")]
macro_rules! console_log {
        ($($args: tt)+) => {{
            let here = format!("{}:{}]", file!(), line!());
            let msg = format!($($args)+);
            let msg_formatted = format!("{} {}", here, msg);
            let msg_js = wasm_bindgen::JsValue::from(msg_formatted);
            web_sys::console::log_1(&msg_js);
        }};
    }

#[cfg(target_arch = "wasm32")]
pub mod wasm {
    use std::panic::{set_hook, PanicInfo};
    use wasm_bindgen::prelude::*;

    /// Set up a panic hook that prints the panic location, the message and the backtrace.
    /// (The default Rust handler doesn't have the means to print the message).
    pub fn set_panic_hook() {
        set_hook(Box::new(|info: &PanicInfo| {
            // let mut trace = String::new();
            // stack_trace(&mut stack_trace_frame, &mut |l| trace.push_str(l));
            console_err!("{}", info);
        }))
    }

    #[wasm_bindgen]
    pub async fn run_test() {
        set_panic_hook();

        let mut devices = trezor_api::find_devices().await.unwrap();

        // take the first device out of devices.
        let device = devices.remove(0);

        let mut trezor = device.connect().unwrap();
        trezor.init_device().await.unwrap();

        // const DER_PATH: &str = "m/44'/0'/0'/1";
        const DER_PATH: &str = "m/44'/141'/0'/0/0";

        // After this you can interact with Trezor device.
        let address = trezor
            .get_komodo_address(&DER_PATH.parse().expect("FromStr"))
            .await
            .expect("get_komodo_address")
            .ack_all()
            .await
            .expect("ack_all");
        console_log!("{}", address);
    }
}

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests {
    // use futures::block_on;
    //
    // #[test]
    // fn it_works() {
    //     let mut devices = trezor_api::find_devices().await.unwrap();
    //
    //     // take the first device out of devices.
    //     let device = devices.remove(0);
    //
    //     let mut trezor = device.connect().unwrap();
    //     trezor.init_device().unwrap();
    //
    //     // const DER_PATH: &str = "m/44'/0'/0'/1";
    //     const DER_PATH: &str = "m/44'/141'/0'/0/0";
    //
    //     // After this you can interact with Trezor device.
    //     let address = trezor.get_komodo_address(
    //         &DER_PATH.parse().expect("FromStr"),
    //     ).expect("get_komodo_address").ack_all().expect("ack_all");
    //     println!("{}", address);
    // }
}
