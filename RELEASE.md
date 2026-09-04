RELEASE_TYPE: patch

This patch improves shrinking for collections whose elements each cost
more than eight choices to generate. Previously such an element could
only be deleted a few choices at a time, so shrunk counterexamples kept
collection elements with no effect on the failure.
