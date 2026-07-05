
/**
 * 文件预览组件
 * 根据文件类型（video/image/audio）渲染对应的预览元素。
 * 统一 BanAppealPage 和 PlayerReportPage 中的重复实现。
 */

export function fileIcon(category) {
  if (category === 'video') return '🎬';
  if (category === 'image') return '🖼';
  if (category === 'audio') return '🎵';
  if (category === 'replay') return '▶';
  if (category === 'demo') return '▣';
  return '📎';
}

export function fileActionLabel(category) {
  if (category === 'image') return '打开原图';
  if (category === 'video') return '播放录像';
  if (category === 'audio') return '播放录音';
  if (category === 'replay') return '查看 Replay';
  if (category === 'demo') return '查看 Demo';
  return '下载文件';
}

export function fileTypeLabel(category) {
  if (category === 'video') return '录像';
  if (category === 'image') return '图片';
  if (category === 'audio') return '录音';
  if (category === 'replay') return 'Replay';
  if (category === 'demo') return 'Demo';
  return '文件';
}

export function formatFileSize(bytes) {
  const value = Number(bytes);
  if (!Number.isFinite(value)) return '-';
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
}

export function FilePreview({ file }) {
  if (!file || !file.url) return null;

  if (file.category === 'video') {
    return (
      <video
        src={file.url}
        controls
        preload="metadata"
        className="file-preview-video"
      >
        当前浏览器不支持播放该视频，请下载原文件查看。
      </video>
    );
  }

  if (file.category === 'audio') {
    return (
      <audio
        src={file.url}
        controls
        preload="metadata"
        className="file-preview-audio"
      >
        当前浏览器不支持播放该音频，请下载原文件查看。
      </audio>
    );
  }

  if (file.category === 'image') {
    return (
      <a href={file.url} target="_blank" rel="noopener noreferrer" className="file-preview-image-link">
        <img
          src={file.url}
          alt={file.file_name || '预览图片'}
          loading="lazy"
          className="file-preview-image"
        />
      </a>
    );
  }

  if (file.category === 'replay' || file.category === 'demo') {
    return (
      <div className="file-preview-replay">
        <div>
          <div className="file-preview-replay-title">{fileTypeLabel(file.category)} 存证文件</div>
          <div className="file-preview-replay-meta">{file.file_name}</div>
        </div>
        <a href={file.url} target="_blank" rel="noopener noreferrer" className="action-btn action-btn-accent">
          {fileActionLabel(file.category)}
        </a>
      </div>
    );
  }

  return null;
}

/**
 * 文件列表项组件（用于申诉/举报详情弹窗）
 */
export function FileItem({ file, children }) {
  return (
    <div className="file-item">
      <div className="file-item-header">
        <div className="file-item-info">
          <span className="file-item-icon" aria-hidden="true">{fileIcon(file.category)}</span>
          <div className="file-item-detail">
            <div className="file-item-name">{file.file_name}</div>
            <div className="file-item-meta">
              {formatFileSize(file.file_size)}
              {file.content_type ? ` · ${file.content_type}` : ''}
            </div>
          </div>
        </div>
        {children}
      </div>
      <FilePreview file={file} />
    </div>
  );
}
