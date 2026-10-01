import { Layer, type MapRef, Map as MapView, Source } from '@vis.gl/react-maplibre';
import type { FeatureCollection } from 'geojson';
import { setWorkerUrl } from 'maplibre-gl';
import 'maplibre-gl/dist/maplibre-gl.css';
import workerUrl from 'maplibre-gl/dist/maplibre-gl-worker.mjs?worker&url';
import { useCallback, useEffect, useMemo, useRef } from 'react';
import { lineColorExpression } from '../activity.ts';
import type { HistoryResponse } from '../api/schema.gen.ts';
import { formatCount } from '../format.ts';
import { trackBounds } from '../geo.ts';
import styles from './TrackMap.module.css';

// MapLibre GL JS 6 は Vite でビルドするとき、Worker の URL を明示する必要がある
setWorkerUrl(workerUrl);

/** OpenFreeMap のベクタータイルのスタイル。登録も API キーも要らない。 */
const STYLE_URL = 'https://tiles.openfreemap.org/styles/liberty';

/** 軌跡が無いときの初期表示 (東京駅)。 */
const INITIAL_VIEW = { longitude: 139.767, latitude: 35.681, zoom: 10 };

type Props = {
  /** 地図に描く軌跡 */
  track: HistoryResponse['track'];
  /** 間引きの情報 */
  meta: HistoryResponse['track_meta'];
};

/** 期間の軌跡を移動種別で色分けして描く地図。 */
export function TrackMap({ track, meta }: Props) {
  const mapRef = useRef<MapRef>(null);
  const bounds = useMemo(() => trackBounds(track), [track]);
  const colors = useMemo(() => lineColorExpression(), []);

  const fit = useCallback(() => {
    if (bounds) {
      mapRef.current?.fitBounds(bounds, { padding: 40, duration: 0, maxZoom: 16 });
    }
  }, [bounds]);

  useEffect(fit, [fit]);

  return (
    <div className={styles.container}>
      <MapView
        ref={mapRef}
        initialViewState={INITIAL_VIEW}
        mapStyle={STYLE_URL}
        onLoad={fit}
        style={{ width: '100%', height: '100%' }}
      >
        <Source id="track" type="geojson" data={track as FeatureCollection}>
          <Layer
            id="track-line"
            type="line"
            layout={{ 'line-join': 'round', 'line-cap': 'round' }}
            paint={{ 'line-color': colors, 'line-width': 3 }}
          />
        </Source>
      </MapView>
      {meta.simplified && (
        <div className={styles.badge}>
          {formatCount(meta.returned_points)} / {formatCount(meta.original_points)}{' '}
          点に間引いて表示中
        </div>
      )}
      {track.features.length === 0 && (
        <div className={styles.empty}>この期間の記録はありません</div>
      )}
    </div>
  );
}
