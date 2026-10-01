import { DayPicker, type DateRange as PickerRange } from '@daypicker/react';
import { ja } from '@daypicker/react/locale';
import '@daypicker/react/style.css';
import { useState } from 'react';
import { type DateRange, fromIsoDate, presetRanges, toIsoDate } from '../range.ts';
import styles from './RangePicker.module.css';

type Props = {
  /** 選んでいる期間 */
  range: DateRange;
  /** 今日 (これより後の日は選べない) */
  today: Date;
  /** 期間を変えたときに呼ぶ */
  onChange: (range: DateRange) => void;
};

/** よく使う範囲のボタンと、カレンダーでの範囲選択。 */
export function RangePicker({ range, today, onChange }: Props) {
  const [draft, setDraft] = useState<PickerRange | undefined>(undefined);
  const open = draft !== undefined;

  const toggle = () =>
    setDraft(open ? undefined : { from: fromIsoDate(range.from), to: fromIsoDate(range.to) });

  const apply = () => {
    if (!draft?.from) {
      return;
    }
    onChange({ from: toIsoDate(draft.from), to: toIsoDate(draft.to ?? draft.from) });
    setDraft(undefined);
  };

  return (
    <div className={styles.picker}>
      {presetRanges(today).map((preset) => (
        <button
          key={preset.label}
          type="button"
          aria-pressed={preset.range.from === range.from && preset.range.to === range.to}
          onClick={() => onChange(preset.range)}
        >
          {preset.label}
        </button>
      ))}
      <button type="button" aria-expanded={open} onClick={toggle}>
        {range.from === range.to ? range.from : `${range.from} 〜 ${range.to}`}
      </button>
      {open && (
        <div className={styles.popover}>
          <DayPicker
            mode="range"
            locale={ja}
            selected={draft}
            onSelect={(next) => setDraft(next ?? { from: undefined })}
            defaultMonth={draft.from}
            disabled={{ after: today }}
          />
          <button type="button" disabled={!draft.from} onClick={apply}>
            この期間を表示
          </button>
        </div>
      )}
    </div>
  );
}
