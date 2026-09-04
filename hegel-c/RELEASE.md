RELEASE_TYPE: patch

This patch adds a new shrink pass that is able to delete regions of the test case where it would previously have got stuck.
You should see improvements in cases where there were previously redundant elements that were "obviously" deletable but that the shrinker was for some reason struggling with.
