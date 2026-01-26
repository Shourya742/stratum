# codec-sv2 Benchmarks

Benchmarking suite for the codec-sv2 crate, covering encoding, decoding, serialization, Noise
protocol encryption/decryption, and buffer pool exhaustion behavior.

## Running Benchmarks

```bash
# All benchmarks (no optional features)
cargo bench

# With Noise protocol support
cargo bench --features noise_sv2

# With buffer pool allocator
cargo bench --features with_buffer_pool

# With all features (recommended for complete picture)
cargo bench --all-features

# Specific suite
cargo bench --bench encoder
cargo bench --bench decoder
cargo bench --bench noise_roundtrip --features noise_sv2
cargo bench --bench serialization
cargo bench --bench buffer_exhaustion
```

## Benchmark Suites

### 1. Encoder (`encoder.rs`)

- **`encoder/plain`** — Single-frame encode with a plain `Encoder<T>`
- **`encoder/creation/plain`** — `Encoder::new()` overhead

With `noise_sv2` feature:
- **`encoder/noise/transport`** — Noise-encrypted frame encoding in transport mode (reuses an established session)
- **`encoder/creation/noise`** — `NoiseEncoder::new()` overhead
- **`encoder/noise/handshake/complete`** — Full 3-step Noise handshake

### 2. Decoder (`decoder.rs`)

- **`decoder/plain`** — Full decode loop: fill writable buffer, call `next_frame()` until complete
- **`decoder/creation/plain`** — `StandardDecoder::new()` overhead

### 3. Noise Roundtrip (`noise_roundtrip.rs`)

Requires `noise_sv2` feature.

- **`noise/roundtrip`** — Complete encode → encrypt → decrypt → decode cycle per iteration (fresh session each time to avoid state reuse)
- **`noise/encode_only`** — Noise encode in isolation with a persistent transport session
- **`noise/handshake/step_0`** — Initiator generates the first EllSwift key-exchange message
- **`noise/handshake/step_1`** — Responder processes step-0 and generates its response

### 4. Serialization (`serialization.rs`)

- **`serialization/frame_from_message`** — `Sv2Frame::from_message()`: stores message as `Option<T>`, no serialization yet
- **`serialization/frame_serialize_and_create`** — Frame creation + `serialize()` to `Vec<u8>`

### 5. Buffer Pool Exhaustion (`buffer_exhaustion.rs`)

Measures latency at each stage of the `BufferPool` state machine
(`Back → Front → Alloc`) and shows where the cost cliff occurs.

## Interpreting Results

Results use [criterion.rs](https://github.com/bheisler/criterion.rs):
- HTML reports in `target/criterion/`
- Each run compares against the previous stored baseline in `target/criterion/`


```rust
cargo bench --all-features --bench noise_roundtrip
warning: profiles for the non root package will be ignored, specify profiles at the workspace root:
package:   /home/shourya/stratum/sv2/noise-sv2/Cargo.toml
workspace: /home/shourya/stratum/Cargo.toml
   Compiling codec_sv2 v5.0.0 (/home/shourya/stratum/sv2/codec-sv2)
    Finished bench [optimized] target(s) in 3.31s
     Running benches/noise_roundtrip.rs (/home/shourya/stratum/target/release/deps/noise_roundtrip-9e01b33091d0f884)
Gnuplot not found, using plotters backend
noise/roundtrip         time:   [679.73 µs 680.98 µs 682.39 µs]                            
                        change: [-1.0430% -0.3167% +0.4471%] (p = 0.41 > 0.05)
                        No change in performance detected.
Found 12 outliers among 100 measurements (12.00%)
  6 (6.00%) high mild
  6 (6.00%) high severe

noise/encode_only       time:   [2.2139 µs 2.2183 µs 2.2236 µs]                               
                        change: [+0.5615% +1.4101% +2.3010%] (p = 0.00 < 0.05)
                        Change within noise threshold.
Found 14 outliers among 100 measurements (14.00%)
  2 (2.00%) high mild
  12 (12.00%) high severe

noise/handshake/step_0  time:   [105.40 µs 105.77 µs 106.14 µs]                                   
                        change: [+6.4737% +7.8729% +9.1578%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high mild

noise/handshake/step_1  time:   [400.03 µs 401.72 µs 403.56 µs]                                   
                        change: [-7.5946% -5.9268% -4.5366%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) high mild

Benchmarking noise/handshake/step_2: Warming up for 3.0000 s[sv2/codec-sv2/benches/noise_roundtrip.rs:220] "ONE" = "ONE"
thread 'main' panicked at sv2/codec-sv2/benches/noise_roundtrip.rs:225:73:
called `Result::unwrap()` on an `Err` value: NoiseSv2Error(AesGcm(Error))
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

error: bench failed, to rerun pass `--bench noise_roundtrip`
shourya@shourya-HP-EliteBook-840-G6:~/stratum/sv2/codec-sv2$ cargo bench --all-features --bench noise_roundtrip
warning: profiles for the non root package will be ignored, specify profiles at the workspace root:
package:   /home/shourya/stratum/sv2/noise-sv2/Cargo.toml
workspace: /home/shourya/stratum/Cargo.toml
   Compiling codec_sv2 v5.0.0 (/home/shourya/stratum/sv2/codec-sv2)
    Finished bench [optimized] target(s) in 2.15s
     Running benches/noise_roundtrip.rs (/home/shourya/stratum/target/release/deps/noise_roundtrip-9e01b33091d0f884)
Gnuplot not found, using plotters backend
noise/roundtrip         time:   [719.37 µs 729.93 µs 740.83 µs]                            
                        change: [+3.0160% +4.3412% +5.6357%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high mild

noise/encode_only       time:   [2.2238 µs 2.2406 µs 2.2598 µs]                               
                        change: [-0.7236% +0.1923% +1.0761%] (p = 0.68 > 0.05)
                        No change in performance detected.
Found 12 outliers among 100 measurements (12.00%)
  7 (7.00%) high mild
  5 (5.00%) high severe

noise/handshake/step_0  time:   [111.02 µs 112.03 µs 113.06 µs]                                   
                        change: [+3.6290% +4.4389% +5.2784%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 8 outliers among 100 measurements (8.00%)
  4 (4.00%) high mild
  4 (4.00%) high severe

noise/handshake/step_1  time:   [420.79 µs 422.30 µs 423.80 µs]                                   
                        change: [+4.0827% +4.6647% +5.2208%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 9 outliers among 100 measurements (9.00%)
  2 (2.00%) low mild
  4 (4.00%) high mild
  3 (3.00%) high severe

shourya@shourya-HP-EliteBook-840-G6:~/stratum/sv2/codec-sv2$ cargo bench --all-features
warning: profiles for the non root package will be ignored, specify profiles at the workspace root:
package:   /home/shourya/stratum/sv2/noise-sv2/Cargo.toml
workspace: /home/shourya/stratum/Cargo.toml
    Finished bench [optimized] target(s) in 0.06s
     Running unittests src/lib.rs (/home/shourya/stratum/target/release/deps/codec_sv2-df775ed71eaf1a96)

running 3 tests
test decoder::tests::unencrypted_writable_with_missing_b_initialized_as_header_size ... ignored
test tests::handshake_step_fails_if_state_is_in_transport_mode ... ignored
test tests::handshake_step_fails_if_state_is_not_initialized ... ignored

test result: ok. 0 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running benches/buffer_exhaustion.rs (/home/shourya/stratum/target/release/deps/buffer_exhaustion-70e377c0dafad0bc)
Gnuplot not found, using plotters backend
encoder/pool_exhaustion/back_mode                                                                            
                        time:   [158.72 ns 173.18 ns 186.76 ns]
                        change: [-25.457% -21.076% -16.207%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 15 outliers among 100 measurements (15.00%)
  2 (2.00%) high mild
  13 (13.00%) high severe
encoder/pool_exhaustion/alloc_mode_after_pool_exhausted                                                                             
                        time:   [328.52 ns 343.26 ns 359.86 ns]
                        change: [+12.567% +18.161% +24.116%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) high mild
encoder/pool_exhaustion/recovery_after_full_release                                                                             
                        time:   [212.19 ns 221.72 ns 232.17 ns]
                        change: [+14.678% +18.818% +23.780%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high mild

encoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/0                                                                            
                        time:   [156.30 ns 159.32 ns 162.98 ns]
                        change: [-32.106% -30.380% -28.637%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 13 outliers among 100 measurements (13.00%)
  6 (6.00%) high mild
  7 (7.00%) high severe
encoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/1                                                                            
                        time:   [153.07 ns 153.58 ns 154.09 ns]
                        change: [-4.2710% -3.7721% -3.2973%] (p = 0.00 < 0.05)
                        Performance has improved.
encoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/2                                                                             
                        time:   [155.02 ns 155.76 ns 156.54 ns]
                        change: [-3.5971% -3.1235% -2.6054%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 3 outliers among 100 measurements (3.00%)
  2 (2.00%) high mild
  1 (1.00%) high severe
encoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/4                                                                             
                        time:   [157.09 ns 160.38 ns 165.03 ns]
                        change: [-12.110% -10.404% -8.4040%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 8 outliers among 100 measurements (8.00%)
  4 (4.00%) high mild
  4 (4.00%) high severe
encoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/6                                                                             
                        time:   [157.71 ns 158.59 ns 159.49 ns]
                        change: [-21.373% -19.241% -17.336%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) high mild
encoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/7                                                                             
                        time:   [155.10 ns 155.60 ns 156.09 ns]
                        change: [-14.594% -13.711% -12.830%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild
encoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/8                                                                             
                        time:   [192.07 ns 193.26 ns 194.58 ns]
                        change: [-20.066% -18.092% -16.342%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 7 outliers among 100 measurements (7.00%)
  4 (4.00%) high mild
  3 (3.00%) high severe
encoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/9                                                                             
                        time:   [183.32 ns 183.88 ns 184.44 ns]
                        change: [-23.196% -21.627% -20.062%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 27 outliers among 100 measurements (27.00%)
  11 (11.00%) low severe
  1 (1.00%) low mild
  6 (6.00%) high mild
  9 (9.00%) high severe
encoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/12                                                                             
                        time:   [182.94 ns 183.57 ns 184.20 ns]
                        change: [-25.802% -24.457% -23.138%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 2 outliers among 100 measurements (2.00%)
  1 (1.00%) high mild
  1 (1.00%) high severe
encoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/16                                                                             
                        time:   [175.69 ns 176.49 ns 177.36 ns]
                        change: [-22.649% -21.208% -19.921%] (p = 0.00 < 0.05)
                        Performance has improved.

decoder/pool_exhaustion/back_mode                                                                             
                        time:   [61.331 ns 61.625 ns 61.929 ns]
                        change: [-12.664% -11.131% -9.6642%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 5 outliers among 100 measurements (5.00%)
  1 (1.00%) high mild
  4 (4.00%) high severe
decoder/pool_exhaustion/alloc_mode_after_pool_exhausted                                                                             
                        time:   [78.733 ns 79.332 ns 79.874 ns]
                        change: [+11.059% +12.863% +14.621%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 24 outliers among 100 measurements (24.00%)
  11 (11.00%) low severe
  4 (4.00%) low mild
  4 (4.00%) high mild
  5 (5.00%) high severe

decoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/0                                                                             
                        time:   [60.441 ns 60.651 ns 60.852 ns]
                        change: [-24.687% -22.098% -19.553%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 4 outliers among 100 measurements (4.00%)
  1 (1.00%) high mild
  3 (3.00%) high severe
decoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/1                                                                             
                        time:   [58.543 ns 58.715 ns 58.970 ns]
                        change: [-15.744% -13.813% -11.938%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 19 outliers among 100 measurements (19.00%)
  7 (7.00%) low severe
  4 (4.00%) high mild
  8 (8.00%) high severe
decoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/2                                                                             
                        time:   [58.543 ns 58.628 ns 58.705 ns]
                        change: [-10.655% -9.0712% -7.3558%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 13 outliers among 100 measurements (13.00%)
  3 (3.00%) low severe
  2 (2.00%) low mild
  8 (8.00%) high severe
decoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/4                                                                             
                        time:   [58.659 ns 58.753 ns 58.870 ns]
                        change: [-19.285% -17.365% -15.412%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 17 outliers among 100 measurements (17.00%)
  11 (11.00%) high mild
  6 (6.00%) high severe
decoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/6                                                                             
                        time:   [58.942 ns 59.107 ns 59.318 ns]
                        change: [-6.3979% -4.7025% -2.8585%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 12 outliers among 100 measurements (12.00%)
  9 (9.00%) high mild
  3 (3.00%) high severe
decoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/7                                                                             
                        time:   [57.833 ns 58.062 ns 58.299 ns]
                        change: [+1.3960% +2.0343% +2.6650%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 4 outliers among 100 measurements (4.00%)
  3 (3.00%) high mild
  1 (1.00%) high severe
decoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/8                                                                             
                        time:   [63.530 ns 63.879 ns 64.346 ns]
                        change: [-2.0900% -1.0651% +0.3139%] (p = 0.08 > 0.05)
                        No change in performance detected.
Found 11 outliers among 100 measurements (11.00%)
  3 (3.00%) low mild
  2 (2.00%) high mild
  6 (6.00%) high severe
decoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/9                                                                             
                        time:   [63.079 ns 63.267 ns 63.504 ns]
                        change: [-2.8428% -2.3479% -1.9117%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 5 outliers among 100 measurements (5.00%)
  4 (4.00%) high mild
  1 (1.00%) high severe
decoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/12                                                                             
                        time:   [63.762 ns 64.408 ns 65.161 ns]
                        change: [-2.0238% -1.2564% -0.3546%] (p = 0.00 < 0.05)
                        Change within noise threshold.
Found 7 outliers among 100 measurements (7.00%)
  1 (1.00%) high mild
  6 (6.00%) high severe
decoder/pool_exhaustion/per_slot_latency/slots_held_before_measure/16                                                                             
                        time:   [62.845 ns 63.012 ns 63.221 ns]
                        change: [-7.4658% -5.9723% -4.3598%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 17 outliers among 100 measurements (17.00%)
  1 (1.00%) high mild
  16 (16.00%) high severe

encoder/zerocopy_pool_exhaustion/back_mode                                                                            
                        time:   [197.99 ns 198.67 ns 199.39 ns]
                        change: [-0.6888% +0.5921% +2.3449%] (p = 0.46 > 0.05)
                        No change in performance detected.
Found 8 outliers among 100 measurements (8.00%)
  5 (5.00%) high mild
  3 (3.00%) high severe
encoder/zerocopy_pool_exhaustion/alloc_mode_after_byte_limit_4_held                                                                             
                        time:   [233.02 ns 233.79 ns 234.50 ns]
                        change: [-8.9913% -7.7691% -6.6291%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 37 outliers among 100 measurements (37.00%)
  19 (19.00%) low severe
  2 (2.00%) low mild
  4 (4.00%) high mild
  12 (12.00%) high severe

encoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/0                                                                            
                        time:   [202.01 ns 203.38 ns 204.97 ns]
                        change: [-12.197% -10.097% -8.1462%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 7 outliers among 100 measurements (7.00%)
  5 (5.00%) high mild
  2 (2.00%) high severe
encoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/1                                                                             
                        time:   [206.11 ns 206.83 ns 207.59 ns]
                        change: [-1.2707% -0.5603% +0.0671%] (p = 0.11 > 0.05)
                        No change in performance detected.
Found 3 outliers among 100 measurements (3.00%)
  2 (2.00%) high mild
  1 (1.00%) high severe
encoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/2                                                                             
                        time:   [210.22 ns 211.24 ns 212.72 ns]
                        change: [-0.2331% +1.8302% +3.7281%] (p = 0.08 > 0.05)
                        No change in performance detected.
Found 28 outliers among 100 measurements (28.00%)
  16 (16.00%) low severe
  3 (3.00%) low mild
  4 (4.00%) high mild
  5 (5.00%) high severe
encoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/3                                                                             
                        time:   [208.71 ns 209.62 ns 210.56 ns]
                        change: [+1.6275% +2.3624% +3.0212%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 31 outliers among 100 measurements (31.00%)
  15 (15.00%) low severe
  1 (1.00%) low mild
  6 (6.00%) high mild
  9 (9.00%) high severe
encoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/4                                                                             
                        time:   [240.37 ns 241.25 ns 242.21 ns]
                        change: [+2.0237% +3.3154% +5.5599%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 35 outliers among 100 measurements (35.00%)
  13 (13.00%) low severe
  3 (3.00%) low mild
  5 (5.00%) high mild
  14 (14.00%) high severe
encoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/5                                                                             
                        time:   [229.26 ns 230.06 ns 230.83 ns]
                        change: [-8.2349% -6.3883% -4.4771%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 38 outliers among 100 measurements (38.00%)
  16 (16.00%) low mild
  13 (13.00%) high mild
  9 (9.00%) high severe
encoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/7                                                                             
                        time:   [230.04 ns 230.73 ns 231.44 ns]
                        change: [-17.088% -14.834% -12.636%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 7 outliers among 100 measurements (7.00%)
  1 (1.00%) low severe
  1 (1.00%) low mild
  3 (3.00%) high mild
  2 (2.00%) high severe
encoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/8                                                                             
                        time:   [222.30 ns 223.90 ns 225.98 ns]
                        change: [-22.562% -20.845% -19.178%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 20 outliers among 100 measurements (20.00%)
  9 (9.00%) low severe
  2 (2.00%) high mild
  9 (9.00%) high severe
encoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/12                                                                             
                        time:   [222.07 ns 222.79 ns 223.51 ns]
                        change: [-26.578% -25.020% -23.140%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 27 outliers among 100 measurements (27.00%)
  11 (11.00%) low mild
  6 (6.00%) high mild
  10 (10.00%) high severe

decoder/zerocopy_pool_exhaustion/back_mode                                                                             
                        time:   [61.579 ns 61.825 ns 62.103 ns]
                        change: [-32.032% -30.763% -29.538%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 7 outliers among 100 measurements (7.00%)
  3 (3.00%) high mild
  4 (4.00%) high severe
decoder/zerocopy_pool_exhaustion/alloc_mode_8_zc_frames_held                                                                             
                        time:   [111.08 ns 111.94 ns 112.93 ns]
                        change: [-34.604% -32.780% -30.956%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 4 outliers among 100 measurements (4.00%)
  1 (1.00%) high mild
  3 (3.00%) high severe

decoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/0                                                                             
                        time:   [61.133 ns 61.401 ns 61.725 ns]
                        change: [-35.255% -34.050% -32.756%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 10 outliers among 100 measurements (10.00%)
  2 (2.00%) high mild
  8 (8.00%) high severe
decoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/1                                                                             
                        time:   [58.708 ns 59.080 ns 59.525 ns]
                        change: [-49.004% -40.316% -33.985%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 5 outliers among 100 measurements (5.00%)
  3 (3.00%) high mild
  2 (2.00%) high severe
decoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/2                                                                             
                        time:   [58.660 ns 58.829 ns 59.034 ns]
                        change: [-10.885% -5.4083% +1.7957%] (p = 0.12 > 0.05)
                        No change in performance detected.
Found 6 outliers among 100 measurements (6.00%)
  2 (2.00%) high mild
  4 (4.00%) high severe
decoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/4                                                                             
                        time:   [58.189 ns 58.296 ns 58.434 ns]
                        change: [-16.179% -14.423% -12.636%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 7 outliers among 100 measurements (7.00%)
  4 (4.00%) high mild
  3 (3.00%) high severe
decoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/6                                                                             
                        time:   [58.984 ns 59.203 ns 59.435 ns]
                        change: [-15.793% -14.612% -13.486%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high mild
decoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/7                                                                             
                        time:   [57.700 ns 57.925 ns 58.159 ns]
                        change: [-19.753% -18.191% -16.675%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 2 outliers among 100 measurements (2.00%)
  1 (1.00%) high mild
  1 (1.00%) high severe
decoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/8                                                                             
                        time:   [118.72 ns 119.34 ns 120.01 ns]
                        change: [-27.574% -26.287% -25.002%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 6 outliers among 100 measurements (6.00%)
  3 (3.00%) high mild
  3 (3.00%) high severe
decoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/9                                                                             
                        time:   [128.22 ns 128.86 ns 129.54 ns]
                        change: [-20.026% -18.695% -17.491%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 7 outliers among 100 measurements (7.00%)
  5 (5.00%) high mild
  2 (2.00%) high severe
decoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/12                                                                             
                        time:   [133.00 ns 133.46 ns 133.90 ns]
                        change: [-3.6400% -1.8563% -0.2388%] (p = 0.03 < 0.05)
                        Change within noise threshold.
Found 30 outliers among 100 measurements (30.00%)
  16 (16.00%) low severe
  3 (3.00%) low mild
  4 (4.00%) high mild
  7 (7.00%) high severe
decoder/zerocopy_pool_exhaustion/per_slot_latency/slots_held_before_measure/16                                                                             
                        time:   [117.14 ns 117.81 ns 118.79 ns]
                        change: [-14.646% -13.386% -12.043%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 12 outliers among 100 measurements (12.00%)
  2 (2.00%) low mild
  6 (6.00%) high mild
  4 (4.00%) high severe

encoder/owned_vs_zerocopy_exhaustion/owned_TestMsg_7B/slots_held/0                                                                            
                        time:   [144.26 ns 144.68 ns 145.15 ns]
                        change: [+1.6620% +2.2611% +2.8438%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 2 outliers among 100 measurements (2.00%)
  1 (1.00%) high mild
  1 (1.00%) high severe
encoder/owned_vs_zerocopy_exhaustion/owned_TestMsg_7B/slots_held/4                                                                             
                        time:   [146.75 ns 147.30 ns 147.86 ns]
                        change: [-17.380% -14.110% -10.943%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 31 outliers among 100 measurements (31.00%)
  18 (18.00%) low severe
  2 (2.00%) low mild
  3 (3.00%) high mild
  8 (8.00%) high severe
encoder/owned_vs_zerocopy_exhaustion/owned_TestMsg_7B/slots_held/7                                                                             
                        time:   [145.02 ns 145.56 ns 146.13 ns]
                        change: [-1.1368% -0.5248% +0.0057%] (p = 0.07 > 0.05)
                        No change in performance detected.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high mild
encoder/owned_vs_zerocopy_exhaustion/owned_TestMsg_7B/slots_held/8                                                                             
                        time:   [181.02 ns 181.71 ns 182.46 ns]
                        change: [-15.354% -13.636% -12.028%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 33 outliers among 100 measurements (33.00%)
  20 (20.00%) low severe
  1 (1.00%) low mild
  1 (1.00%) high mild
  11 (11.00%) high severe
encoder/owned_vs_zerocopy_exhaustion/owned_TestMsg_7B/slots_held/9                                                                             
                        time:   [169.18 ns 169.84 ns 170.51 ns]
                        change: [-15.579% -14.570% -13.644%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) high mild
encoder/owned_vs_zerocopy_exhaustion/zerocopy_ZeroCopyMsg_108B/slots_held/0                                                                            
                        time:   [210.91 ns 211.55 ns 212.17 ns]
                        change: [-24.056% -22.881% -21.797%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 30 outliers among 100 measurements (30.00%)
  11 (11.00%) low severe
  1 (1.00%) low mild
  2 (2.00%) high mild
  16 (16.00%) high severe
encoder/owned_vs_zerocopy_exhaustion/zerocopy_ZeroCopyMsg_108B/slots_held/3                                                                             
                        time:   [214.64 ns 218.87 ns 223.27 ns]
                        change: [-33.460% -30.873% -28.423%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 15 outliers among 100 measurements (15.00%)
  6 (6.00%) high mild
  9 (9.00%) high severe
encoder/owned_vs_zerocopy_exhaustion/zerocopy_ZeroCopyMsg_108B/slots_held/4                                                                             
                        time:   [251.17 ns 255.03 ns 259.70 ns]
                        change: [-15.091% -12.538% -9.8565%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) high mild
encoder/owned_vs_zerocopy_exhaustion/zerocopy_ZeroCopyMsg_108B/slots_held/5                                                                             
                        time:   [231.60 ns 232.35 ns 233.04 ns]
                        change: [-20.592% -18.436% -16.663%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 26 outliers among 100 measurements (26.00%)
  12 (12.00%) low severe
  5 (5.00%) high mild
  9 (9.00%) high severe
encoder/owned_vs_zerocopy_exhaustion/zerocopy_ZeroCopyMsg_108B/slots_held/8                                                                             
                        time:   [223.94 ns 225.06 ns 226.49 ns]
                        change: [-10.495% -8.0958% -5.7648%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 11 outliers among 100 measurements (11.00%)
  2 (2.00%) low mild
  5 (5.00%) high mild
  4 (4.00%) high severe

encoder/zerocopy_payload_size_vs_exhaustion/back_mode/coinbase_bytes/16                                                                            
                        time:   [209.98 ns 212.60 ns 215.75 ns]
                        change: [-19.341% -17.324% -15.406%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 9 outliers among 100 measurements (9.00%)
  5 (5.00%) high mild
  4 (4.00%) high severe
encoder/zerocopy_payload_size_vs_exhaustion/alloc_mode/coinbase_bytes/16                                                                             
                        time:   [243.90 ns 246.41 ns 249.68 ns]
                        change: [-10.975% -8.3828% -5.7766%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 8 outliers among 100 measurements (8.00%)
  4 (4.00%) high mild
  4 (4.00%) high severe
encoder/zerocopy_payload_size_vs_exhaustion/back_mode/coinbase_bytes/64                                                                            
                        time:   [206.89 ns 207.37 ns 208.02 ns]
                        change: [-17.802% -16.625% -15.621%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 15 outliers among 100 measurements (15.00%)
  4 (4.00%) high mild
  11 (11.00%) high severe
encoder/zerocopy_payload_size_vs_exhaustion/alloc_mode/coinbase_bytes/64                                                                             
                        time:   [240.96 ns 241.78 ns 242.60 ns]
                        change: [-10.541% -7.9178% -5.3103%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 26 outliers among 100 measurements (26.00%)
  18 (18.00%) low mild
  4 (4.00%) high mild
  4 (4.00%) high severe
encoder/zerocopy_payload_size_vs_exhaustion/back_mode/coinbase_bytes/128                                                                            
                        time:   [208.13 ns 208.89 ns 209.80 ns]
                        change: [-26.472% -24.379% -22.363%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 5 outliers among 100 measurements (5.00%)
  4 (4.00%) high mild
  1 (1.00%) high severe
encoder/zerocopy_payload_size_vs_exhaustion/alloc_mode/coinbase_bytes/128                                                                             
                        time:   [242.43 ns 243.31 ns 244.22 ns]
                        change: [-16.317% -14.416% -12.684%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 29 outliers among 100 measurements (29.00%)
  16 (16.00%) low mild
  6 (6.00%) high mild
  7 (7.00%) high severe
encoder/zerocopy_payload_size_vs_exhaustion/back_mode/coinbase_bytes/200                                                                            
                        time:   [200.69 ns 201.25 ns 201.92 ns]
                        change: [-30.736% -29.627% -28.534%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 14 outliers among 100 measurements (14.00%)
  13 (13.00%) high mild
  1 (1.00%) high severe
encoder/zerocopy_payload_size_vs_exhaustion/alloc_mode/coinbase_bytes/200                                                                             
                        time:   [242.47 ns 243.32 ns 244.15 ns]
                        change: [-22.902% -22.044% -21.246%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 2 outliers among 100 measurements (2.00%)
  1 (1.00%) high mild
  1 (1.00%) high severe

     Running benches/common.rs (/home/shourya/stratum/target/release/deps/common-f6122b249c481f3d)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running benches/decoder.rs (/home/shourya/stratum/target/release/deps/decoder-754f6df588ff6c15)
Gnuplot not found, using plotters backend
decoder/plain           time:   [57.077 ns 57.298 ns 57.535 ns]                          
                        change: [-39.306% -37.747% -36.220%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 32 outliers among 100 measurements (32.00%)
  12 (12.00%) low mild
  16 (16.00%) high mild
  4 (4.00%) high severe

decoder/creation/plain  time:   [6.2317 µs 6.3138 µs 6.4011 µs]                                    
                        change: [-46.256% -45.401% -44.498%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 7 outliers among 100 measurements (7.00%)
  5 (5.00%) high mild
  2 (2.00%) high severe

     Running benches/encoder.rs (/home/shourya/stratum/target/release/deps/encoder-ffdd72d91f5aa88e)
Gnuplot not found, using plotters backend
encoder/plain           time:   [135.68 ns 135.92 ns 136.21 ns]                          
                        change: [-38.922% -36.791% -34.772%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high severe

encoder/creation/plain  time:   [108.72 ns 109.13 ns 109.55 ns]                                   
                        change: [-31.728% -30.795% -29.905%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 33 outliers among 100 measurements (33.00%)
  12 (12.00%) low severe
  2 (2.00%) low mild
  3 (3.00%) high mild
  16 (16.00%) high severe

encoder/noise/transport time:   [2.3673 µs 2.3773 µs 2.3871 µs]                                     
                        change: [-36.950% -35.162% -33.345%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild

encoder/creation/noise  time:   [12.976 µs 13.033 µs 13.090 µs]                                    
                        change: [-23.725% -21.933% -20.141%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 3 outliers among 100 measurements (3.00%)
  2 (2.00%) high mild
  1 (1.00%) high severe

encoder/noise/handshake/complete                                                                            
                        time:   [715.07 µs 718.96 µs 723.02 µs]
                        change: [-34.062% -32.894% -31.808%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 5 outliers among 100 measurements (5.00%)
  4 (4.00%) high mild
  1 (1.00%) high severe

     Running benches/noise_roundtrip.rs (/home/shourya/stratum/target/release/deps/noise_roundtrip-9e01b33091d0f884)
Gnuplot not found, using plotters backend
noise/roundtrip         time:   [757.93 µs 760.14 µs 762.41 µs]                            
                        change: [+6.0635% +7.5226% +8.9748%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 4 outliers among 100 measurements (4.00%)
  2 (2.00%) high mild
  2 (2.00%) high severe

noise/encode_only       time:   [2.3686 µs 2.3747 µs 2.3815 µs]                               
                        change: [+6.7042% +7.6120% +8.5673%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 4 outliers among 100 measurements (4.00%)
  1 (1.00%) high mild
  3 (3.00%) high severe

noise/handshake/step_0  time:   [105.39 µs 105.85 µs 106.31 µs]                                   
                        change: [-4.9755% -3.6535% -2.1488%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high severe

noise/handshake/step_1  time:   [400.50 µs 401.94 µs 403.32 µs]                                   
                        change: [-5.2591% -4.4055% -3.1437%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high severe

     Running benches/serialization.rs (/home/shourya/stratum/target/release/deps/serialization-48f522b76a791c38)
Gnuplot not found, using plotters backend
serialization/frame_from_message                                                                             
                        time:   [6.7868 ns 6.8097 ns 6.8330 ns]
                        change: [+194.44% +200.39% +206.07%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 4 outliers among 100 measurements (4.00%)
  4 (4.00%) high severe

serialization/frame_serialize_and_create                                                                            
                        time:   [123.04 ns 123.38 ns 123.72 ns]
                        change: [-10.353% -8.4177% -6.4961%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 4 outliers among 100 measurements (4.00%)
  2 (2.00%) high mild
  2 (2.00%) high severe
```

```bash
# Save a named baseline
cargo bench --bench encoder -- --save-baseline main

# Compare against it
cargo bench --bench encoder -- --baseline main

# Target a single bench
cargo bench --bench buffer_exhaustion -- encoder/pool_exhaustion/per_slot_latency

# Faster iteration (fewer samples)
cargo bench --bench encoder -- --sample-size 20
```
