# 63-row denominator evidence

Candidate Extension `0.3.21` (`259532b91d9f7db8b9a610a17c24223e1bf2189e96810ca3354b45bc083056cd`)
passed the complete local denominator in clean Chrome for Testing 151 and
Microsoft Edge 151 profiles.

| Result | Rows |
| --- | ---: |
| Local pass | 56 |
| Truthful limitation | 7 |
| Local blocked | 0 |
| Publication blocked | 63 |

The seven truthful limitations are `opaque_surface`, `restricted_document`,
reserved `unknown` non-emission, observation-only `drop_target`, built-in PDF,
restricted frame, and closed Shadow DOM. These are successful boundary checks,
not missing implementation.

The report combines the 15 interactive Control modules, 30 semantic and variant
targets, frame/Shadow/opaque/push boundaries, and 11 page-driven lifecycle
scenarios. Chrome and Edge use the same Extension candidate. A local result
does not change the public denominator's publication outcome; public pages and
client-owned same-tab execution remain separate release evidence.

Evidence: `20260813T224810Z/denominator-63.json`. The Truth and lifecycle inputs
share that evidence root and were produced by one `./scripts/dev.sh denominator`
run.
