import { useCallback, useState } from 'react';
import { Modal } from './Modal.jsx';
import { createAlertOptions, createConfirmOptions } from './confirmDialog.js';

export function ConfirmModal({ options, onCancel, onConfirm }) {
  if (!options) return null;

  const confirmClassName = options.tone === 'danger' ? 'btn btn-danger' : 'btn btn-primary';

  return (
    <Modal
      open
      title={options.title}
      onClose={onCancel ?? onConfirm}
      footer={(
        <>
          {onCancel ? <button className="btn btn-outline" type="button" onClick={onCancel}>{options.cancelText}</button> : null}
          <button className={confirmClassName} type="button" onClick={onConfirm}>{options.confirmText}</button>
        </>
      )}
    >
      <div className="confirm-dialog-message">{options.message}</div>
    </Modal>
  );
}

export function useConfirmDialog() {
  const [request, setRequest] = useState(null);

  const confirm = useCallback((options) => new Promise((resolve) => {
    setRequest({ kind: 'confirm', options: createConfirmOptions(options), resolve });
  }), []);

  const alert = useCallback((options) => new Promise((resolve) => {
    setRequest({ kind: 'alert', options: createAlertOptions(options), resolve });
  }), []);

  const close = useCallback((value) => {
    setRequest((current) => {
      current?.resolve(value);
      return null;
    });
  }, []);

  const dialog = (
    <ConfirmModal
      options={request?.options ?? null}
      onCancel={request?.kind === 'confirm' ? () => close(false) : null}
      onConfirm={() => close(true)}
    />
  );

  return { confirm, alert, dialog };
}
