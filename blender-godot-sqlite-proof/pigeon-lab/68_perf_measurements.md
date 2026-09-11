
## /private/tmp/pigeon-perf-before.VdlEuz/tracing.jsonl

Inclusive busy milliseconds. Nested rows overlap. Debug + synchronous tracing overhead.

| Span | Calls | Total ms | Mean ms | Max ms |
| --- | ---: | ---: | ---: | ---: |
| pigeon::runtime::advance | 540 | 2983.350 | 5.525 | 80.800 |
| runtime bucket: other ticks | 537 | 2926.330 | 5.449 | 80.800 |
| pigeon::rollback::peer | 1080 | 2919.220 | 2.703 | 61.100 |
| pigeon::snapshot::physics_clone | 2564 | 2736.096 | 1.067 | 46.000 |
| pigeon::verification::physics_equal | 803 | 937.030 | 1.167 | 3.320 |
| pigeon::physics::physics_step | 1298 | 120.664 | 0.093 | 5.760 |
| runtime bucket: delivery tick 97 | 3 | 57.020 | 19.007 | 27.500 |
| pigeon::sql::read_frame | 200 | 23.437 | 0.117 | 0.209 |
| pigeon::sql::publish | 180 | 14.010 | 0.078 | 0.148 |
| pigeon::rollback::restore | 2 | 1.820 | 0.910 | 0.916 |
| pigeon::physics::launch | 7 | 0.180 | 0.026 | 0.159 |
| pigeon::sql::reader_for | 1 | 0.064 | 0.064 | 0.064 |

## /private/tmp/pigeon-perf.YLoOpz/tracing.jsonl

Inclusive busy milliseconds. Nested rows overlap. Debug + synchronous tracing overhead.

| Span | Calls | Total ms | Mean ms | Max ms |
| --- | ---: | ---: | ---: | ---: |
| pigeon::verification::physics_equal | 803 | 864.926 | 1.077 | 2.820 |
| pigeon::runtime::advance | 540 | 399.692 | 0.740 | 4.390 |
| runtime bucket: other ticks | 537 | 389.492 | 0.725 | 4.250 |
| pigeon::rollback::peer | 1080 | 350.872 | 0.325 | 3.450 |
| pigeon::physics::physics_step | 1298 | 85.848 | 0.066 | 0.652 |
| pigeon::snapshot::physics_clone | 2564 | 35.375 | 0.014 | 0.258 |
| pigeon::sql::read_frame | 200 | 24.954 | 0.125 | 0.531 |
| pigeon::sql::publish | 180 | 14.668 | 0.081 | 0.410 |
| runtime bucket: delivery tick 97 | 3 | 10.200 | 3.400 | 4.390 |
| pigeon::rollback::restore | 2 | 0.117 | 0.059 | 0.063 |
| pigeon::sql::reader_for | 1 | 0.049 | 0.049 | 0.049 |
| pigeon::physics::launch | 7 | 0.019 | 0.003 | 0.005 |

## /private/tmp/pigeon-perf.RKQ6F3/tracing.jsonl

Inclusive busy milliseconds. Nested rows overlap. Debug + synchronous tracing overhead.

| Span | Calls | Total ms | Mean ms | Max ms |
| --- | ---: | ---: | ---: | ---: |
| pigeon::verification::physics_equal | 803 | 845.204 | 1.053 | 1.710 |
| pigeon::runtime::advance | 540 | 394.511 | 0.731 | 6.840 |
| runtime bucket: other ticks | 537 | 385.583 | 0.718 | 6.840 |
| pigeon::rollback::peer | 1080 | 347.520 | 0.322 | 6.180 |
| pigeon::physics::physics_step | 1298 | 87.342 | 0.067 | 2.920 |
| pigeon::snapshot::physics_clone | 2564 | 33.964 | 0.013 | 1.760 |
| pigeon::sql::read_frame | 200 | 19.858 | 0.099 | 0.158 |
| pigeon::sql::publish | 180 | 11.608 | 0.064 | 0.127 |
| runtime bucket: delivery tick 97 | 3 | 8.928 | 2.976 | 4.290 |
| pigeon::rollback::restore | 2 | 0.116 | 0.058 | 0.066 |
| pigeon::sql::reader_for | 1 | 0.037 | 0.037 | 0.037 |
| pigeon::physics::launch | 7 | 0.017 | 0.002 | 0.004 |
