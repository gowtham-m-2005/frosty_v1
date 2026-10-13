/**
 * @frosty/sdk: what a capability imports.
 *
 *   import { defineCapability } from "@frosty/sdk";
 *   import { useTool, useEvent } from "@frosty/sdk/views";
 */
import type { CapabilityDefinition, ViewsDefinition } from "./types";

export type {
  Bridge,
  CapabilityDefinition,
  Ctx,
  Db,
  FrostyEvent,
  Json,
  QueryOptions,
  Row,
  ScheduleOptions,
  ScheduledAction,
  ToolHandler,
  Unsubscribe,
  ViewProps,
  ViewsDefinition,
} from "./types";
export { matchesPattern } from "./types";

/** The logic entry: `export default defineCapability({...})`. */
export function defineCapability(definition: CapabilityDefinition): CapabilityDefinition {
  return definition;
}

/** The views entry: `export default defineViews({...})`. */
export function defineViews(views: ViewsDefinition): ViewsDefinition {
  return views;
}
