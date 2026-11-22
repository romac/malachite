parse:
	find . -name '*.qnt' | ../../../scripts/quint-forall.sh parse
.PHONY: parse

test:
	find tests -name '*Test.qnt' | ../../../scripts/quint-forall.sh test --max-samples=1000
.PHONY: test

verify:
	quint verify --max-steps 10 \
		--main votekeeperModels --init=VK1::init --step=VK1::step \
		--invariant VK1::Inv \
		./tests/votekeeper/votekeeperModels.qnt
.PHONY: verify

# Generate traces that end at each requested vote-keeper output.
traces-sim:
	./scripts/gen-traces.sh tests/votekeeper/voteReplaySampling.qnt emitPrecommitValue 30
	./scripts/gen-traces.sh tests/votekeeper/voteReplaySampling.qnt emitPolkaAny 30
	./scripts/gen-traces.sh tests/votekeeper/voteReplaySampling.qnt emitPolkaNil 30
	./scripts/gen-traces.sh tests/votekeeper/voteReplaySampling.qnt emitSkip 30
.PHONY: traces-sim

# Generate traces from `run` statements.
traces-run-vk:
	mkdir -p traces/votekeeper
	quint test --out-itf 'traces/votekeeper/{test}.itf.json' ./tests/votekeeper/votekeeperTest.qnt
.PHONY: traces-run-vk

traces-run-con:
	mkdir -p traces/consensus
	quint test --out-itf 'traces/consensus/{test}.itf.json' ./tests/consensus/consensusTest.qnt
.PHONY: traces-run-con

traces-run: traces-run-vk traces-run-con
.PHONY: traces-run

traces: traces-sim traces-run
.PHONY: traces
