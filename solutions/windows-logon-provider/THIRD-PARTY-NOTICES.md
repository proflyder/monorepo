# Third-party notices

## Microsoft Windows-classic-samples

Parts of this directory are adapted from the Microsoft
[Windows-classic-samples](https://github.com/microsoft/Windows-classic-samples)
repository:

- `Samples/CredentialProvider/cpp`
  (`helpers.cpp`, `helpers.h` — Kerberos logon packing, `CredProtect`,
  `FieldDescriptorCoAllocCopy`; `CSampleProvider.cpp`, `CSampleCredential.cpp`,
  `Dll.cpp`, `common.h` — interface shapes and the credential-provider
  boilerplate);
- `Samples/Win7Samples/security/credentialproviders/samplehardwareeventcredentialprovider`
  (the `Advise` / `ICredentialProviderEvents::CredentialsChanged`
  re-enumeration pattern).

The C++ sources are gone; the provider is Rust. The Kerberos logon packing
adapted from `helpers.cpp` lives in `crates/provider/src/kerb.rs`, which keeps
the original copyright header. The remaining files were reduced to the
behaviour this provider needs and substantially rewritten.

```
The MIT License (MIT)

Copyright (c) Microsoft Corporation

Permission is hereby granted, free of charge, to any person obtaining a copy
 of this software and associated documentation files (the "Software"), to deal
 in the Software without restriction, including without limitation the rights
 to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 copies of the Software, and to permit persons to whom the Software is
 furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
 all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 THE SOFTWARE.
```

## zig

The Windows cross-build links with the hermetic [zig](https://ziglang.org/)
toolchain fetched by Bazel (`platform/bazel/zig/`); it is not distributed with
this repository. Zig ships the mingw-w64/UCRT runtime components under their
own licences, and its compiler is MIT licensed.
