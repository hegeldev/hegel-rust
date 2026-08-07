RELEASE_TYPE: patch

This patch fixes a bias in data generation where some choices were made with the wrong probability.
The most visible effect should be the stateful tests should run a full set of steps more often.
Collection sizes may also be affected.
