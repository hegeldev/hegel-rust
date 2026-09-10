RELEASE_TYPE: minor

This release removes the `hegel_label_t` enum of predefined span labels from `hegel.h`, and with it the idea that a span label means anything. A label is now an opaque `uint64_t` identifying the generator that opened the span: libhegel treats two spans with the same label as coming from the same generator — candidates for swapping, duplicating and reordering when it shrinks and mutates test cases — and does nothing else with it. This is how Hypothesis has always treated labels, and it means a new kind of generator no longer needs a new ABI constant.

Two functions derive labels, so every binding derives them the same way:

```c
uint64_t list_kind, element;
hegel_label_from_name(ctx, "mylib.list", &list_kind);
hegel_label_from_name(ctx, "mylib.integers", &element);
uint64_t parts[2] = {list_kind, element};
uint64_t list_of_integers;
hegel_label_combine(ctx, parts, 2, &list_of_integers);
```

`hegel_label_from_name` is the 64-bit FNV-1a hash of the name's bytes, so a binding may equally compute labels ahead of time. `hegel_label_combine` hashes a sequence of labels into one; passing a generator's own label followed by its components' labels gives `lists(integers())` and `lists(text())` different labels while every `lists(integers())` gets the same one, which is what lets the engine tell them apart.

Bindings that passed `HEGEL_LABEL_*` constants to `hegel_start_span` should replace each with a label derived from a name of their own choosing, prefixed with the binding's name to keep clear of libhegel's `hegel.<kind>` names, and should give each generator built from other generators a label combined from its components'. libhegel's own spans around its draws are now labelled the same way, from names such as `hegel.integer` and `hegel.feature_flag`; nothing about them was ever part of the ABI.
