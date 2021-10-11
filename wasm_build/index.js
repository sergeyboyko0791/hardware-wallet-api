import "core-js/stable";
import "regenerator-runtime/runtime";
import init, {run_test} from "./deps/pkg/wasm_hw_test.js";

// Loads the wasm file, so we use the
// default export to inform it where the wasm file is located on the
// server, and then we wait on the returned promise to wait for the
// wasm to be loaded.
async function init_wasm() {
    try {
        await init();
    } catch (e) {
        alert(`Oops: ${e}`);
    }
}

async function on_user_action() {
    await run_test()
}

init_wasm();

document.querySelector('#my-button').addEventListener("click", () => on_user_action(), false);
