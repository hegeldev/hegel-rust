RELEASE_TYPE: patch

This patch adds a shrink pass that deletes the region between two
occurrences of a repeated run of choice values. The existing deletion
passes only try windows of up to eight choices, so a collection element
costing more than that could never be deleted, and shrunk collections
kept elements with no effect on the failure. Value shrinking makes
sibling elements identical, and the repeats then mark the element
boundaries, so whole elements can be deleted at any width.
