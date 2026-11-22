//! Test entry: renders the Drivers screen with a fake store so a failed
//! download's persistent error state can be asserted without network access.
import { mount } from "nsh";
import { Drivers } from "../screens/Drivers";

const failing = {
  title: "Failing driver",
  kind: "driver" as const,
  url: "https://www.colorfulgroup.com/bios/failing-driver",
  file_name: "failing-driver",
  published: "2026-01-02",
};

const store = {
  drivers: {
    list: async () => ({
      board: "Test Board",
      product_id: 1,
      installed_bios: "1.0.0",
      files: [{ ...failing, newer: false }],
      fetched_at_ms: 0,
      cached: true,
      warning: null,
    }),
    stored: async () => [],
    reveal: async () => {},
    download: async () => {
      throw new Error("network unreachable");
    },
  },
};

mount(() => <Drivers store={store as never} toast={() => {}} />);
