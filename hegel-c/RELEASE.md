RELEASE_TYPE: minor

This patch moves the lifecycle of stateful tests into the engine. Frontends no longer draw a step cap up front; instead, they poll for rules from the engine until they receive a termination signal. This is necessary groundwork for future work on concurrent stateful testing and better shrinking.
