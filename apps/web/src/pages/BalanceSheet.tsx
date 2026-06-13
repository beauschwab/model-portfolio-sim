/** Book browser/editor: per-book tables with JSON round-trip editing. */
import { useEffect, useState } from "react";
import { api, type BookName, type Row } from "../lib/api";
import { Button, Card, CardBody, CardHeader, DataTable, Tabs } from "../components/ui";

const BOOK_TABS: BookName[] = ["mbs", "loans", "debt", "deposits", "cds", "mm"];

export default function BalanceSheet() {
  const [tab, setTab] = useState<BookName>("mbs");
  const [rows, setRows] = useState<Row[]>([]);
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState("");

  const load = (b: BookName) => api.book(b).then(setRows).catch(() => setRows([]));
  useEffect(() => { load(tab); }, [tab]);

  const save = async () => {
    try {
      await api.putBook(tab, JSON.parse(text));
      setEditing(false);
      load(tab);
    } catch (e) { alert(String(e)); }
  };

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <Tabs tabs={BOOK_TABS} active={tab} onChange={t => setTab(t as BookName)} />
        <div className="flex gap-2">
          {!editing
            ? <Button variant="ghost" onClick={() => { setText(JSON.stringify(rows, null, 1)); setEditing(true); }}>Edit book (JSON)</Button>
            : <>
                <Button onClick={save}>Save</Button>
                <Button variant="ghost" onClick={() => setEditing(false)}>Cancel</Button>
              </>}
        </div>
      </div>
      <Card>
        <CardHeader title={`${tab} — ${rows.length} positions`}
          sub="balances sized to the WFC 1Q26 mix (synthetic; see model_balance_sheet docstring)" />
        <CardBody className="p-0">
          {editing
            ? <textarea className="h-[28rem] w-full bg-surface-2 p-3 font-mono text-[11px] text-paper-dim outline-none" value={text} onChange={e => setText(e.target.value)} />
            : <DataTable rows={rows} />}
        </CardBody>
      </Card>
    </div>
  );
}
