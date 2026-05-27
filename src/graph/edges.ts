import { z } from "zod";

export const GraphEdgeSchema = z.object({
  from: z.string().regex(/^N\d{4,}$/),
  to: z.string().regex(/^N\d{4,}$/),
  type: z.string(),
});

export type GraphEdge = z.infer<typeof GraphEdgeSchema>;
