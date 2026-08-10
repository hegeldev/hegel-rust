RELEASE_TYPE: patch

This patch improves the generation phase's span mutation. A mutated choice sequence that diverges from its donor's path previously ran out of data and was discarded as an overrun; mutation probes now draw randomly past the end of the spliced choices, so a diverged proposal becomes a complete test case seeded with the mutation. On recursive-generator workloads this turns a substantial fraction of previously wasted probes into productive test cases.
