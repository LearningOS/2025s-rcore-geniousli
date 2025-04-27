# chapter6练习

# 问答作业： 
1. 实验总结: 实现了hard link，并在 DiskNode中增加了link_cnt 字段。在unlink的时候 link_cnt == 0 时，清理inode对应的content: File/Dir, 释放 inode
1. root inode 起着主索引的作用，其内容记录者所有的name，node_id 键值对，如果损坏了就无法在通过name对文件进行定位

# chapter7练习

# 问答作业： 
1. pipe 用于连接 父子 进程， 程序a | b, a作为写入端，将数据通过管道传递给 b进行处理。cat file | wc -l
2. 设计为 广播形式。任何一个进程把持 广播的写入端，同时把持 监听端。 广播需要 在写入端 写入特殊的消息目的地，如果需要广播给所有进程 则手动写入多条信息
