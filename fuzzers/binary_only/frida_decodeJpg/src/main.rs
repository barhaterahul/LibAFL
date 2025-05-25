// use frida::{DeviceManager, Frida};

use frida_gum::{Gum, NativePointer, interceptor::Interceptor};
use std::{marker::PhantomData, sync::LazyLock};
// static FRIDA: LazyLock<Frida> = LazyLock::new(|| unsafe { Frida::obtain() });
static GUM: LazyLock<Gum> = LazyLock::new(Gum::obtain);

type FnType = fn();

fn hello() {
    println!("hello")
}

fn hook_hello() {
    let mut ctx = Interceptor::current_invocation();
    let data = ctx.replacement_data().unwrap().0;
    hello()
}

struct HookData {
    old_fn: Option<FnType>,
}

fn main() {
    let mut interceptor = Interceptor::obtain(&GUM);
    let r = interceptor
        .replace(
            NativePointer(hello as _),
            NativePointer(hook_hello as _),
            NativePointer(std::ptr::null_mut()),
        )
        .unwrap();

    println!(
        "replace :{:p} hello: {:p} hook_hello: {:p}",
        r.0, hello as FnType, hook_hello as FnType
    );
    hello();
    interceptor.revert(NativePointer(hello as _));
}
