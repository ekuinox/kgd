// scripts/gen-api.ts が src/api/schema.json から生成する。手で編集しないこと。
import * as v from 'valibot';

export const ActivityKindSchema = v.picklist(["walking","cycling","automotive","stationary","unknown"]);
export type ActivityKind = v.InferOutput<typeof ActivityKindSchema>;

export const DistanceByActivitySchema = v.object({ "automotive": v.number(), "cycling": v.number(), "unknown": v.number(), "walking": v.number() });
export type DistanceByActivity = v.InferOutput<typeof DistanceByActivitySchema>;

export const HistorySummarySchema = v.object({ "distance_by_activity": DistanceByActivitySchema, "distance_m": v.number(), "excluded_count": v.pipe(v.number(), v.integer(), v.minValue(0)), "first_at": v.optional(v.nullable(v.pipe(v.string(), v.isoTimestamp()))), "last_at": v.optional(v.nullable(v.pipe(v.string(), v.isoTimestamp()))), "moving_s": v.pipe(v.number(), v.integer()), "point_count": v.pipe(v.number(), v.integer(), v.minValue(0)), "stationary_s": v.pipe(v.number(), v.integer()) });
export type HistorySummary = v.InferOutput<typeof HistorySummarySchema>;

export const DailySummarySchema = v.object({ "date": v.pipe(v.string(), v.isoDate()), "summary": HistorySummarySchema });
export type DailySummary = v.InferOutput<typeof DailySummarySchema>;

export const ErrorResponseSchema = v.object({ "error": v.string() });
export type ErrorResponse = v.InferOutput<typeof ErrorResponseSchema>;

export const FeatureCollectionTypeSchema = v.picklist(["FeatureCollection"]);
export type FeatureCollectionType = v.InferOutput<typeof FeatureCollectionTypeSchema>;

export const FeatureTypeSchema = v.picklist(["Feature"]);
export type FeatureType = v.InferOutput<typeof FeatureTypeSchema>;

export const HistoryQuerySchema = v.object({ "from": v.pipe(v.string(), v.isoDate()), "to": v.pipe(v.string(), v.isoDate()) });
export type HistoryQuery = v.InferOutput<typeof HistoryQuerySchema>;

export const HistoryRangeSchema = v.object({ "from": v.pipe(v.string(), v.isoDate()), "timezone": v.string(), "to": v.pipe(v.string(), v.isoDate()) });
export type HistoryRange = v.InferOutput<typeof HistoryRangeSchema>;

export const LineStringTypeSchema = v.picklist(["LineString"]);
export type LineStringType = v.InferOutput<typeof LineStringTypeSchema>;

export const LineStringSchema = v.object({ "coordinates": v.array(v.pipe(v.array(v.number()), v.minLength(2), v.maxLength(2))), "type": LineStringTypeSchema });
export type LineString = v.InferOutput<typeof LineStringSchema>;

export const TrackPropertiesSchema = v.object({ "activity": ActivityKindSchema });
export type TrackProperties = v.InferOutput<typeof TrackPropertiesSchema>;

export const TrackFeatureSchema = v.object({ "geometry": LineStringSchema, "properties": TrackPropertiesSchema, "type": FeatureTypeSchema });
export type TrackFeature = v.InferOutput<typeof TrackFeatureSchema>;

export const TrackSchema = v.object({ "features": v.array(TrackFeatureSchema), "type": FeatureCollectionTypeSchema });
export type Track = v.InferOutput<typeof TrackSchema>;

export const TrackMetaSchema = v.object({ "original_points": v.pipe(v.number(), v.integer(), v.minValue(0)), "returned_points": v.pipe(v.number(), v.integer(), v.minValue(0)), "simplified": v.boolean() });
export type TrackMeta = v.InferOutput<typeof TrackMetaSchema>;

export const HistoryResponseSchema = v.object({ "days": v.array(DailySummarySchema), "range": HistoryRangeSchema, "total": HistorySummarySchema, "track": TrackSchema, "track_meta": TrackMetaSchema });
export type HistoryResponse = v.InferOutput<typeof HistoryResponseSchema>;

export const ViewerApiSchema = v.object({ "error_response": ErrorResponseSchema, "history_query": HistoryQuerySchema, "history_response": HistoryResponseSchema });
export type ViewerApi = v.InferOutput<typeof ViewerApiSchema>;
