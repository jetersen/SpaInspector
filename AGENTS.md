# Public distribution boundary

Keep SPA reading, metadata preservation, plotting, original-data exports, and
viewer/control code public. Do not add numerical correction implementations,
reprocessing algorithms, native numerical oracle arrays, processing benchmarks,
or processing reconstruction research to this repository.

Public builds must not depend on private repositories or credentials. Run
`python3 tools/check-public-boundary.py` before publishing source, packages,
browser bundles, or desktop artifacts. Decode stored processing metadata without
reimplementing the processing itself.

Push only the intended public branch. Do not use `git push --all` or `--mirror`:
local research/checkpoint refs may contain material outside the public boundary.
