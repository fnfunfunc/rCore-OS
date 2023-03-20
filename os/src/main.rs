#![feature(panic_info_message)]
#![feature(alloc_error_handler)]
#![no_std]
#![no_main]

extern crate alloc;
#[macro_use]
extern crate bitflags;

use core::arch::global_asm;


#[path = "boards/qemu.rs"]
mod board;
mod config;
#[macro_use]
mod console;
mod lang_items;
mod loader;
mod mm;
mod sbi;
mod sync;
pub mod syscall;
mod task;
mod timer;
mod trap;


global_asm!(include_str!("entry.asm"));
global_asm!(include_str!("link_app.S"));

fn clear_bss() {
    extern "C" {
        fn sbss();
        fn ebss();
    }
    unsafe {
        core::slice::from_raw_parts_mut(sbss as usize as *mut u8, ebss as usize - sbss as usize)
            .fill(0);
    }
}

#[no_mangle]
pub fn rust_main() -> ! {
    clear_bss();
    println!("[kernel] Hello, world!");
    mm::init();
    mm::remap_test();
    task::add_initproc();
    println!("after initproc!");
    trap::init();
    //trap::enable_interrupt();
    trap::enable_timer_interrupt();
    timer::set_next_trigger();
    loader::list_apps();
    task::run_tasks();
    panic!("Unreachable in rust_main!");
}