
export function PageHeader({ breadcrumb, title, subtitle, action }) {
  return (
    <>
      <div className="breadcrumb">{breadcrumb}</div>
      <div className="page-header">
        <div>
          <h1>{title}</h1>
          <p>{subtitle}</p>
        </div>
        {action}
      </div>
    </>
  );
}
