use core::arch::global_asm;
use riscv::register::mtvec::TrapMode;
use riscv::register::{scause, sie, stval, stvec};
use riscv::register::scause::Interrupt;
use riscv::register::scause::{Exception, Trap};
use crate::syscall::syscall;
pub use context::TrapContext;
use crate::task::suspend_current_and_run_next;
use crate::timer::set_next_trigger;

mod context;

global_asm!(include_str!("trap.asm"));

pub fn init() {
    extern "C" {
        fn __alltraps();
    }
    unsafe {
        // 将陷入的入口地址设置为 __alltraps
        stvec::write(__alltraps as usize, TrapMode::Direct);
    }
}


#[no_mangle]
pub fn trap_handler(cx: &mut TrapContext) -> &mut TrapContext {
    let scause = scause::read();
    let stval = stval::read();
    match scause.cause() {
        Trap::Exception(Exception::UserEnvCall) => {
            cx.sepc += 4;
            cx.x[10] = syscall(cx.x[17], [cx.x[10], cx.x[11], cx.x[12]]) as usize;
        }
        Trap::Exception(Exception::StoreFault) | Trap::Exception(Exception::StorePageFault) => {
            println!("[kernel] PageFault in application, bad addr = {:#x}, bad instruction = {:#x}, kernel killed it.", stval, cx.sepc);
            panic!("[kernel] Cannot continue!");
        }
        Trap::Exception(Exception::IllegalInstruction) => {
            println!("[kernel] IllegalInstruction in application, kernel killed it.");
            panic!("[kernel] Cannot continue!");
        }
        Trap::Interrupt(Interrupt::SupervisorTimer) => {
            set_next_trigger();
            suspend_current_and_run_next();
        }
        _ => {
            panic!("Unsupported trap {:?}, stval = {:#x}!", scause.cause(), stval);
        }
    };
    cx
}

pub fn enable_timer_interrupt() {
    unsafe { sie::set_stimer(); }
}