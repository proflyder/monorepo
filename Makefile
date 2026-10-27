.PHONY: test test-e2e coverage bazel-build bazel-test

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
