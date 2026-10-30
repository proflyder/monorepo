.PHONY: test test-e2e coverage bazel-build bazel-test test-deploy-control ci-affected ci-units

BAZEL ?= bazel
BASE ?= origin/develop
HEAD ?= HEAD

test:
	./platform/quality/run-tests.sh

test-e2e:
	./platform/quality/run-e2e.sh

coverage:
	./platform/quality/coverage.sh

bazel-build:
	$(BAZEL) build //...

bazel-test:
	$(BAZEL) test //...

# Mocked-kubectl control regression. No Bazel build is needed: the test passes
# pre-resolved digest-pinned images through the environment.
test-deploy-control:
	bash projects/home/kubernetes/scheduler/test_deploy_control.sh

# Which CI units and platform workflows a change range selects.
ci-affected:
	./platform/ci/affected.sh --base $(BASE) --head $(HEAD) --format text

# Validate the unit map and prove every tracked source/test file is routed.
ci-units:
	python3 platform/ci/check_units.py
