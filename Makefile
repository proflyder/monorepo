.PHONY: test test-e2e coverage bazel-build bazel-test

BAZEL ?= bazel

test:
	./tests/run.sh

test-e2e:
	./tests/run_e2e.sh

coverage:
	./tests/coverage.sh

bazel-build:
	$(BAZEL) build //...

bazel-test:
	$(BAZEL) test //...
