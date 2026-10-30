.PHONY: test test-e2e coverage bazel-build bazel-test test-deploy-control

BAZEL ?= bazel

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

# Builds the static linux/amd64 release binaries that deploy.sh --skip-build
# expects, then runs the mocked-kubectl control regression. CI runs this in the
# bazel job because `make test` only runs it when dist/ already exists.
test-deploy-control:
	$(BAZEL) build \
		//solutions/scheduler/cmd/scheduler \
		//solutions/ha-bridge/cmd/ha-bridge \
		//solutions/ha-bridge/cmd/ha-control \
		--platforms=@rules_go//go/toolchain:linux_amd64
	mkdir -p projects/home/kubernetes/scheduler/dist
	cp -f bazel-bin/solutions/scheduler/cmd/scheduler/scheduler_/scheduler projects/home/kubernetes/scheduler/dist/scheduler
	cp -f bazel-bin/solutions/ha-bridge/cmd/ha-bridge/ha-bridge_/ha-bridge projects/home/kubernetes/scheduler/dist/ha-bridge
	cp -f bazel-bin/solutions/ha-bridge/cmd/ha-control/ha-control_/ha-control projects/home/kubernetes/scheduler/dist/ha-control
	REQUIRE_DEPLOY_CONTROL=1 bash projects/home/kubernetes/scheduler/test_deploy_control.sh
