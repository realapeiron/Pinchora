import { it, expect, vi } from "vitest";
import { api } from "../../../lib/api";
import { createEconomyActionSender } from "./playerEconomy";
vi.mock("../../../lib/api", () => ({ api: vi.fn() }));
it("coalesces double clicks and reuses receipts after an uncertain response", async () => {
  let fail!: (e: Error) => void;
  vi.mocked(api).mockImplementationOnce(
    () =>
      new Promise((_, reject) => {
        fail = reject;
      }),
  );
  const send = createEconomyActionSender();
  const command = { action: "donate", guild: "123", amount: 100 };
  const first = send(command);
  expect(send(command)).toBe(first);
  const assertion = expect(first).rejects.toThrow("lost response");
  fail(new Error("lost response"));
  await assertion;
  vi.mocked(api).mockResolvedValue({ amount: -100 });
  await send({ ...command, amount: 200 });
  await send(command);
  const bodies = vi.mocked(api).mock.calls.map((c) => c[2] as { request_id: string });
  expect(bodies[0]!.request_id).toBe(bodies[2]!.request_id);
  expect(bodies[0]!.request_id).not.toBe(bodies[1]!.request_id);
  await send(command);
  expect((vi.mocked(api).mock.calls[3]![2] as { request_id: string }).request_id).not.toBe(
    bodies[0]!.request_id,
  );
});
