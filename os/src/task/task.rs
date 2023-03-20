//! Types related to task management
use alloc::sync::{Weak, Arc};
use alloc::vec::Vec;
use core::cell::RefMut;
use super::TaskContext;
use crate::config::{TRAP_CONTEXT};
use crate::mm::{MemorySet, PhysPageNum, VirtAddr, KERNEL_SPACE};
use crate::sync::UPSafeCell;
use crate::task::pid::{KernelStack, pid_alloc, PidHandle};
use crate::trap::{trap_handler, TrapContext};

/// task control block structure
pub struct TaskControlBlock {
    pub pid: PidHandle,
    pub kernel_stack: KernelStack,
    pub inner: UPSafeCell<TaskControlBlockInner>,
}

pub struct TaskControlBlockInner {
    pub trap_cx_ppn: PhysPageNum,
    pub base_size: usize,
    pub task_cx: TaskContext,
    pub task_status: TaskStatus,
    pub memory_set: MemorySet,
    pub parent: Option<Weak<TaskControlBlock>>,
    pub children: Vec<Arc<TaskControlBlock>>,
    pub exit_code: i32,
}

impl TaskControlBlock {
    pub fn inner_exclusive_access(&self) -> RefMut<'_, TaskControlBlockInner> {
        self.inner.exclusive_access()
    }

    pub fn new(elf_data: &[u8]) -> Self {
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT).into())
            .unwrap()
            .ppn();
        let task_status = TaskStatus::Ready;
        // alloc a pid and a kernel stack in kernel space
        let pid_handle = pid_alloc();
        let kernel_stack = KernelStack::new(&pid_handle);
        let kernel_stack_top = kernel_stack.get_top();
        // push a task context which goes to trap_return to the top of kernel stack
        let task_control_block = Self {
            pid: pid_handle,
            kernel_stack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ppn,
                    base_size: user_sp,
                    task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                    task_status,
                    memory_set,
                    parent: None,
                    children: Vec::new(),
                    exit_code: 0,
                })
            },
        };
        // prepare TrapContext in user space
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            kernel_stack_top,
            trap_handler as usize,
        );
        task_control_block
    }

    pub fn getpid(&self) -> usize {
        self.pid.0
    }

    pub fn exec(&self, elf_data: &[u8]) {
        let (memory_set, user_stack_top, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set.translate(VirtAddr::from(TRAP_CONTEXT).into()).unwrap().ppn();

        let mut inner = self.inner_exclusive_access();
        inner.memory_set = memory_set;
        // update trap_cx_ppn
        inner.trap_cx_ppn = trap_cx_ppn;
        // initialize trap_cx
        let trap_cx = inner.get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_stack_top,
            KERNEL_SPACE.exclusive_access().token(),
            self.kernel_stack.get_top(),
            trap_handler as usize
        );
    }

    pub fn fork(self: &Arc<TaskControlBlock>) -> Arc<TaskControlBlock> {
        let mut parent_inner = self.inner_exclusive_access();

        let child_memory_set = MemorySet::from_existing_user(&parent_inner.memory_set);
        let trap_cx_ppn = child_memory_set.translate(VirtAddr::from(TRAP_CONTEXT).into()).unwrap().ppn();
        let pid_handle = pid_alloc();
        let kernel_stack = KernelStack::new(&pid_handle);
        let kernel_stack_top = kernel_stack.get_top();
        let child_task_control_block = Arc::new(TaskControlBlock {
            pid: pid_handle,
            kernel_stack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ppn,
                    base_size: parent_inner.base_size,
                    task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                    task_status: TaskStatus::Ready,
                    memory_set: child_memory_set,
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                })
            },
        });
        // add child
        parent_inner.children.push(child_task_control_block.clone());
        // modify kernel_sp in trap_cx
        let trap_cx = child_task_control_block.inner_exclusive_access().get_trap_cx();
        trap_cx.kernel_sp = kernel_stack_top;
        child_task_control_block
    }

    // /// change the location of the program break. return None if failed.
    // pub fn change_program_brk(&mut self, size: i32) -> Option<usize> {
    //     let old_break = self.program_brk;
    //     let new_brk = self.program_brk as isize + size as isize;
    //     if new_brk < self.heap_bottom as isize {
    //         return None;
    //     }
    //     let result = if size < 0 {
    //         self.memory_set
    //             .shrink_to(VirtAddr(self.heap_bottom), VirtAddr(new_brk as usize))
    //     } else {
    //         self.memory_set
    //             .append_to(VirtAddr(self.heap_bottom), VirtAddr(new_brk as usize))
    //     };
    //     if result {
    //         self.program_brk = new_brk as usize;
    //         Some(old_break)
    //     } else {
    //         None
    //     }
    // }
}

impl TaskControlBlockInner {
    pub fn get_trap_cx(&self) -> &'static mut TrapContext {
        self.trap_cx_ppn.get_mut()
    }

    pub fn get_user_token(&self) -> usize {
        self.memory_set.token()
    }

    fn get_status(&self) -> TaskStatus {
        self.task_status
    }

    pub fn is_zombie(&self) -> bool {
        self.task_status == TaskStatus::Zombie
    }
}

#[derive(Copy, Clone, PartialEq)]
/// task status: UnInit, Ready, Running, Exited
pub enum TaskStatus {
    Ready,
    Running,
    Zombie,
}