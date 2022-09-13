```bash
cargo install cargo-audit
```

```
> cargo audit
    Fetching advisory database from `https://github.com/RustSec/advisory-db.git`
      Loaded 457 security advisories (from /Users/username/.cargo/advisory-db)
    Updating crates.io index
    Scanning Cargo.lock for vulnerabilities (139 crate dependencies)
Crate:     time
Version:   0.1.44
Title:     Potential segfault in the time crate
Date:      2020-11-18
ID:        RUSTSEC-2020-0071
URL:       https://rustsec.org/advisories/RUSTSEC-2020-0071
Solution:  Upgrade to >=0.2.23
Dependency tree:
time 0.1.44
└── chrono 0.4.20
    └── near-primitives 0.13.0
        ├── near-vm-logic 0.13.0
        │   └── near-sdk 4.0.0
        │       ├── zebec 1.0.0
        │       └── near-contract-standards 4.0.0
        │           └── zebec 1.0.0
        └── near-sdk 4.0.0

Crate:     wee_alloc
Version:   0.4.5
Warning:   unmaintained
Title:     wee_alloc is Unmaintained
Date:      2022-05-11
ID:        RUSTSEC-2022-0054
URL:       https://rustsec.org/advisories/RUSTSEC-2022-0054
Dependency tree:
wee_alloc 0.4.5
└── near-sdk 4.0.0
    ├── zebec 1.0.0
    └── near-contract-standards 4.0.0
        └── zebec 1.0.0

error: 1 vulnerability found!
warning: 1 allowed warning found
```