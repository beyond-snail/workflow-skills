import React from 'react';

export function ComposerModal({ draft, onDraftChange, error, onClose, onSubmit }) {
  return (
    <div className="composer-overlay" onMouseDown={onClose}>
      <form className="composer-modal panel" onMouseDown={(event) => event.stopPropagation()} onSubmit={onSubmit}>
        <div className="composer-modal__head">
          <div>
            <p className="eyebrow">新建项目</p>
            <h2>只填项目地址</h2>
            <p>系统会根据路径自动生成项目名、简称和基础卡片。</p>
          </div>
          <button className="ghost-button" type="button" onClick={onClose}>
            关闭
          </button>
        </div>

        <div className="field">
          <label>项目地址</label>
          <input
            value={draft.sourcePath}
            onChange={(event) => onDraftChange({ ...draft, sourcePath: event.target.value })}
            placeholder="例如 /Users/wucongpeng/Documents/jty-work/erp-finance"
          />
        </div>

        {error ? <p className="composer-error">{error}</p> : null}

        <button className="submit-button" type="submit">
          创建项目
        </button>
      </form>
    </div>
  );
}

