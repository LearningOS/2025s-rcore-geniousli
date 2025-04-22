//! Types related to task management

use super::TaskContext;

/// The task control block (TCB) of a task.
#[derive(Copy, Clone)]
pub struct TaskControlBlock {
    /// The task status in it's lifecycle
    pub task_status: TaskStatus,
    /// The task context
    pub task_cx: TaskContext,
    /// syscall counter
    pub syscall_counter: [usize; 500],
}

/// The status of a task
#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    /// uninitialized
    UnInit,
    /// ready to run
    Ready,
    /// running
    Running,
    /// exited
    Exited,
}

impl TaskControlBlock {
    /// syscall is < 500
    pub fn incr_syscall_counter(&mut self, syscall: usize) {
        if syscall >= 500 {
            panic!("syscall bigger than 500");
        } else {
            self.syscall_counter[syscall] += 1;
        }
    }

    /// syscall is < 500
    pub fn get_syscall_counter(&mut self, syscall: usize) -> usize {
        return self.syscall_counter[syscall];
    }
}
