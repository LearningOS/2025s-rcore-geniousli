# chapter8练习

# 问答作业： 
1. 资源包括： 文件描述符(fd_tables), 内存(memory_set + thread kernel stack,),  资源(sem, mutex). 
  其他线程的 TaskControlBlock 可能在TaskManager, CondvarInner: waiter_queue, MutexBlockingInner: wait_queue, 应该都需要释放吧，即便是在 Mutexblockinginner 的wait_queue 中 也可能再次会被add_task
 
2. 第二种是错误的实现，会导致  mutex.lock 没有成功的话， 如果再次被唤醒，不会在loop中继续尝试，而是返回给用户 0，获取锁成功的返回数值。

