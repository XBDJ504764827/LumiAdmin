import React from 'react';

export function DataTable({ headers, rows }) {
  return (
    <div className="table-wrap">
      <table>
        <thead>
          <tr>{headers.map((h) => <th key={h}>{h}</th>)}</tr>
        </thead>
        <tbody>
          {rows.length ? rows.map((row, idx) => <tr key={idx}>{row.map((cell, cidx) => <td key={cidx}>{cell}</td>)}</tr>) : <tr><td colSpan={headers.length}>暂无数据</td></tr>}
        </tbody>
      </table>
    </div>
  );
}
