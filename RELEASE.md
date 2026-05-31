RELEASE_TYPE: patch

This patch improves the performance of value generation on the native engine. Generator schemas are now built directly as CBOR values instead of being encoded to a byte buffer and decoded back, removing an allocation and a full encode/decode round-trip from the construction of each schema. The effect is broadest for integer- and float-heavy tests, where the schema is rebuilt on every draw.
