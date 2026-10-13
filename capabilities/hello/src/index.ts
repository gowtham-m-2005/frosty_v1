import { defineCapability } from "@frosty/sdk";

interface Settings {
  greeting?: string;
  shout?: boolean;
  repeat?: number;
  nicknames?: string[];
}

export default defineCapability({
  id: "hello",

  async onActivate(ctx) {
    const refreshBadge = async () => {
      const rows = await ctx.db.query("greetings");
      await ctx.views.setBadge(rows.length);
    };

    await ctx.tools.register("hello.greet", async ({ name }: { name: string }) => {
      const settings = (await ctx.config.get()) as Settings;
      const word = settings.greeting ?? "Hello";
      const times = Math.max(1, Math.min(5, settings.repeat ?? 1));
      let text = Array.from({ length: times }, () => `${word}, ${name}`).join(". ");
      if (settings.nicknames?.length) text += ` (and ${settings.nicknames.join(", ")})`;
      if (settings.shout) text = text.toUpperCase();

      const row = await ctx.db.insert("greetings", { name, text });
      await ctx.events.emit("hello.greeted", { id: row.id, text });
      await refreshBadge();
      return { id: row.id, text };
    });

    await ctx.tools.register("hello.list", () =>
      ctx.db.query("greetings", { orderBy: "created_at", descending: true, limit: 20 }),
    );

    await ctx.tools.register("hello.clear", async () => {
      const rows = await ctx.db.query("greetings");
      for (const row of rows) await ctx.db.softDelete("greetings", row.id);
      await refreshBadge();
      return { cleared: rows.length };
    });

    await ctx.tools.register("hello.ping", () => ({ pong: true }));

    await refreshBadge();
    await ctx.log.info("hello is ready");
  },

  async onDeactivate(ctx) {
    await ctx.views.setBadge(0);
  },
});
