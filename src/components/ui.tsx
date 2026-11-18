import { X } from 'lucide-react';
import { useEffect, useRef, type ReactNode } from 'react';

export function Modal({
  title,
  subtitle,
  onClose,
  children,
  footer,
  size,
  icon,
}: {
  title: ReactNode;
  subtitle?: ReactNode;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  size?: 'wide' | 'medium';
  icon?: ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener('keydown', onKey, true);
    const first = ref.current?.querySelector<HTMLElement>('[data-autofocus], input, select, textarea');
    first?.focus();
    return () => window.removeEventListener('keydown', onKey, true);
  }, [onClose]);
  return (
    <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className={`modal ${size ?? ''}`} role="dialog" aria-modal="true" ref={ref}>
        <div className="modal-header">
          {icon}
          <div className="col" style={{ gap: 2, minWidth: 0 }}>
            <h2>{title}</h2>
            {subtitle && <p className="muted small">{subtitle}</p>}
          </div>
          <div className="spacer" />
          <button className="btn btn-ghost btn-icon" onClick={onClose} aria-label="Close">
            <X size={18} />
          </button>
        </div>
        <div className="modal-body">{children}</div>
        {footer && <div className="modal-footer">{footer}</div>}
      </div>
    </div>
  );
}

export function Progress({
  value,
  total,
  indeterminate,
}: {
  value: number;
  total: number;
  indeterminate?: boolean;
}) {
  const pct = total > 0 ? Math.min(100, (value / total) * 100) : 0;
  return (
    <div
      className={`progress ${indeterminate ? 'indeterminate' : ''}`}
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={total}
      aria-valuenow={value}
    >
      <div style={{ width: `${pct}%` }} />
    </div>
  );
}

export function Ring({ value, total, label }: { value: number; total: number; label?: string }) {
  const pct = total > 0 ? Math.round((value / total) * 100) : 100;
  return (
    <div className="ring" style={{ ['--p' as string]: pct }} title={label}>
      <span>{pct}%</span>
    </div>
  );
}

export function EmptyState({
  icon,
  title,
  children,
  actions,
}: {
  icon: ReactNode;
  title: string;
  children?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <div className="empty">
      <div className="icon">{icon}</div>
      <h2>{title}</h2>
      {children && <p>{children}</p>}
      {actions && (
        <div className="row" style={{ marginTop: 8 }}>
          {actions}
        </div>
      )}
    </div>
  );
}

export function Stat({
  value,
  label,
  tone,
}: {
  value: ReactNode;
  label: ReactNode;
  tone?: 'warning' | 'danger' | 'success';
}) {
  return (
    <div className={`card stat ${tone ? `tone-${tone}` : ''}`}>
      <span className="value">{value}</span>
      <span className="label">{label}</span>
    </div>
  );
}

export function Callout({
  tone,
  icon,
  children,
}: {
  tone?: 'warning' | 'danger' | 'success';
  icon: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className={`callout ${tone ?? ''}`}>
      {icon}
      <div>{children}</div>
    </div>
  );
}
