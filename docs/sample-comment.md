<!-- soroban-gauge:v1 -->
## soroban-gauge: ❌ 2 regressions

3 of 3 entries changed.

| Function | Instructions Δ | Memory Δ | Fee Δ | Writes Δ | Status |
|---|---:|---:|---:|---:|---|
| `counter_basic/increment#2` | +196,481 (+85.9%) ❌ | +8,444 (+0.7%) | +16,700 (+385.1%) ❌ | +4 (+400.0%) ❌ | regressed |
| `counter_basic/increment#1` | +188,920 (+83.7%) ❌ | +9,517 (+0.8%) | +41,237 (+516.6%) ❌ | +4 (+400.0%) ❌ | regressed |
| `counter_basic/get` | +6,462 (+2.9%) | +989 (+0.1%) | +5 (+3.2%) | 0 | unchanged |

<details><summary>Full metric breakdown</summary>

**`counter_basic/increment#2`** — regressed

| Metric | Baseline | Current | Δ | % |
|---|---:|---:|---:|---:|
| instructions | 228,654 | 425,135 | +196,481 ❌ | +85.9% |
| mem_bytes | 1,133,512 | 1,141,956 | +8,444 | +0.7% |
| disk_read_entries | 0 | 0 | 0 | 0.0% |
| memory_read_entries | 2 | 6 | +4 ❌ | +200.0% |
| write_entries | 1 | 5 | +4 ❌ | +400.0% |
| disk_read_bytes | 0 | 0 | 0 | 0.0% |
| write_bytes | 132 | 496 | +364 ❌ | +275.8% |
| events_bytes | 0 | 0 | 0 | 0.0% |
| fee_stroops | 4,337 | 21,037 | +16,700 ❌ | +385.1% |

**`counter_basic/increment#1`** — regressed

| Metric | Baseline | Current | Δ | % |
|---|---:|---:|---:|---:|
| instructions | 225,679 | 414,599 | +188,920 ❌ | +83.7% |
| mem_bytes | 1,132,998 | 1,142,515 | +9,517 | +0.8% |
| disk_read_entries | 0 | 0 | 0 | 0.0% |
| memory_read_entries | 2 | 6 | +4 ❌ | +200.0% |
| write_entries | 1 | 5 | +4 ❌ | +400.0% |
| disk_read_bytes | 0 | 0 | 0 | 0.0% |
| write_bytes | 132 | 496 | +364 ❌ | +275.8% |
| events_bytes | 0 | 0 | 0 | 0.0% |
| fee_stroops | 7,982 | 49,219 | +41,237 ❌ | +516.6% |

**`counter_basic/get`** — unchanged

| Metric | Baseline | Current | Δ | % |
|---|---:|---:|---:|---:|
| instructions | 219,551 | 226,013 | +6,462 | +2.9% |
| mem_bytes | 1,132,161 | 1,133,150 | +989 | +0.1% |
| disk_read_entries | 0 | 0 | 0 | 0.0% |
| memory_read_entries | 2 | 2 | 0 | 0.0% |
| write_entries | 0 | 0 | 0 | 0.0% |
| disk_read_bytes | 0 | 0 | 0 | 0.0% |
| write_bytes | 0 | 0 | 0 | 0.0% |
| events_bytes | 0 | 0 | 0 | 0.0% |
| fee_stroops | 154 | 159 | +5 | +3.2% |

</details>

Wasm size: `counter` 4,048 B -> 4,142 B (+94 B)

<sub>Estimates from soroban-env-host 29.0.1 / protocol 29 on a local host VM, not exact network fees. See docs/limitations.md.</sub>
