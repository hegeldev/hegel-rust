RELEASE_TYPE: patch

This patch fixes the engine's novel-prefix exploration treating every
recorded boolean draw as a fair coin, regardless of the probability it was
actually drawn with. Stateful tests were most affected: the per-step stop
decision is a very rare draw, but exploration flipped it about half the
time, so roughly half of all generated stateful test cases were truncated
well before their step target.
