module streamReplayTest {
  import part_stream.* from "./part_stream"

  // Rank each permutation once by its first, second, and third selection.
  pure def order(rank) = {
    val slots = [0, 1, 2, 3]
    val first = slots[rank / 6]
    val three = slots.select(x => x != first)
    val second = three[(rank / 2) % 3]
    val two = three.select(x => x != second)
    [first, second, two[rank % 2], two[1 - rank % 2]]
  }

  run allDeliveryOrdersTest = init.then(24.reps(rank =>
    init.then(4.reps(i =>
      processMessage(orderedMessages[order(rank)[i]]).expect(inv)
    )).expect(isDone)))
}
