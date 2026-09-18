/** Poll resolved browser values; CLI wait --fn does not poll async predicates. */
export async function waitFor<T>(read: () => T | Promise<T>, ready: (value: T) => boolean): Promise<T> {
  const deadline = Date.now() + 5000
  for (;;) {
    const value = await read()
    if (ready(value)) return value
    if (Date.now() >= deadline) throw new Error(`Browser condition timed out: ${JSON.stringify(value)}`)
    await Bun.sleep(16)
  }
}
