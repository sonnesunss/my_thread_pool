# Implement My ThreadPool



## Usage



## Debug macro -- log!

实现调试宏，用以在开启debug_log feature时打印调试信息


## 改进

1. 真正的并行

使用线程安全、无锁的channel替换当前有锁竞争的SyncChannel

在有锁channel中，其实是会串行执行的，线程之间是串行拿到锁执行的，类似于: t1 getlock -> t2 getlock -> -> t3 getlock 这样子执行
改成无锁、线程安全的channel则可以避免这些，Arc<Mutex<>>删掉替换即可

2. 优先级支持

3. 线程有返回
