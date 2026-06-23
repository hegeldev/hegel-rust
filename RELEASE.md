RELEASE_TYPE: minor

This release:

- changes the `ip_addresses()` generator to return `std::net::IpAddr` instead of `String`
- changes the `IpAddressGenerator` builder methods (`v4()` and `v6()`) to return new `Ipv4AddressGenerator`/`Ipv6AddressGenerator` types so that they can be used to implement `DefaultGenerator`
- implements `DefaultGenerator` for `std::net::IpAddr`, `std::net::Ipv4Addr` and `std::net::Ipv6Addr`
