/**
 * Composition root: the only place that names the native adapter. Everything
 * below receives `AppStore`, whose commands are use cases bound to the port.
 */

import { Controller } from "opencolorhub/native";

import type { AppStore } from "./store";
import { createStore } from "./store";
import type { ColorHubPort } from "../application/ports";
import { createNativePort } from "../infrastructure/native";

export interface AppDeps {
  controller: Controller;
  port: ColorHubPort;
  store: AppStore;
}

export function createDeps(): AppDeps {
  const controller = new Controller();
  const port = createNativePort(controller);
  const store = createStore(port);
  void store.start();
  return { controller, port, store };
}
