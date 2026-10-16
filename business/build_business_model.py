"""Builds rates-workbench-business-model.xlsx (illustrative business model).

Run: python business/build_business_model.py, then recalculate in Excel/LibreOffice.
"""
from openpyxl import Workbook
from openpyxl.styles import Font, PatternFill, Alignment, Border, Side
from openpyxl.comments import Comment
from openpyxl.utils import get_column_letter as L

wb = Workbook()
BLUE = Font(name="Arial", color="0000FF")
BLK = Font(name="Arial")
GRN = Font(name="Arial", color="008000")
BOLD = Font(name="Arial", bold=True)
H1 = Font(name="Arial", bold=True, size=14)
YEL = PatternFill("solid", fgColor="FFFF00")
HDR = PatternFill("solid", fgColor="D9E1F2")
M = '$#,##0.00;($#,##0.00);-'
N1 = '#,##0.0;(#,##0.0);-'
N0 = '#,##0;(#,##0);-'
P = '0.0%;(0.0%);-'
X = '0.0x'
YRS = ["Y1", "Y2", "Y3", "Y4", "Y5"]
COLS = ["C", "D", "E", "F", "G"]

def put(ws, ref, v, font=BLK, fmt=None, fill=None, note=None):
    c = ws[ref]; c.value = v; c.font = font
    if fmt: c.number_format = fmt
    if fill: c.fill = fill
    if note: c.comment = Comment(note, "model")
    return c

# ---------------- Assumptions ----------------
A = wb.active; A.title = "Assumptions"
put(A, "A1", "Rates Workbench – business model assumptions", H1)
put(A, "A2", "Blue = editable input; yellow = key lever. All $ in $M. Figures are illustrative judgment, not market data.", Font(name="Arial", italic=True))
scal = [
 (5, "Scenario (1=Downside, 2=Base, 3=Upside)", 2, N0, True, "Selects the deal/volume multiplier below"),
 (6, "Volume multiplier – Downside", 0.5, X, False, "Applied to new Large/Mid banks and all channel volumes"),
 (7, "Volume multiplier – Base", 1.0, X, False, None),
 (8, "Volume multiplier – Upside", 1.4, X, False, None),
 (11, "License – Large tier ($50–250B), $M", 0.8, M, True, "Realized after discount; list ~$1.0M"),
 (12, "License – Mid tier ($10–50B), $M", 0.35, M, True, None),
 (13, "License – design partner, $M", 0.25, M, False, None),
 (14, "Share of license paid in year of sale", 0.8, P, False, "50% at working runs + 50% at validation approval or month 6; ~20% slips to next year"),
 (15, "Onboarding + validation support – Large, $M", 0.25, M, False, "1-month integration + validation support package"),
 (16, "Onboarding + validation support – Mid, $M", 0.15, M, False, None),
 (17, "Onboarding + validation support – design partner, $M", 0.15, M, False, None),
 (18, "Update subscription, % of license per year", 0.2, P, False, "Starts year after sale"),
 (19, "Design-partner subscription, $M/yr", 0.15, M, False, None),
 (20, "Ongoing customization – Large, $M/yr", 0.15, M, False, None),
 (21, "Ongoing customization – Mid, $M/yr", 0.075, M, False, None),
 (22, "Ongoing customization – design partner, $M/yr", 0.15, M, False, None),
 (31, "Outsourced-ALM partner platform fee, $M/yr", 0.35, M, False, None),
 (32, "Royalty per partner-served institution, $M/yr", 0.008, '$#,##0.000', True, "~$8k; community banks pay outsourced ALM ~$15–60k/yr"),
 (33, "Validation-firm challenger license, $M/yr", 0.15, M, False, None),
 (34, "M&A / purchase-accounting engagement fee, $M", 0.10, M, False, None),
 (35, "Advisory (Big 4) enterprise license, $M/yr", 0.5, M, False, None),
 (37, "Fully loaded cost per head, $M/yr", 0.275, M, True, None),
 (38, "Commission on direct license bookings", 0.10, P, False, None),
 (39, "Commission on channel revenue", 0.05, P, False, None),
 (41, "Pool: US regional banks $10–250B (count)", 150, N0, False, "Approximate"),
 (42, "Pool: US community banks + credit unions <$10B (count)", 8500, N0, False, "Approximate"),
 (43, "US bank M&A deals per year (count)", 150, N0, False, "Approximate"),
]
put(A, "A4", "Scalar inputs", BOLD); put(A, "C4", "Value", BOLD)
put(A, "A10", "Direct pricing", BOLD); put(A, "A23", "Delivery effort", BOLD)
put(A, "A30", "Channel pricing", BOLD); put(A, "A36", "Costs", BOLD); put(A, "A40", "Market pools", BOLD)
for r, lab, v, f, key, note in scal:
    put(A, f"A{r}", lab)
    put(A, f"C{r}", v, BLUE, f, YEL if key else None, note)
put(A, "A3", "Operating model (1=Standard FDE team, 2=Lean agent foundry)")
put(A, "C3", 2, BLUE, N0, YEL, "Lean: agent pods do integration/validation drafting; one FDE supervises several banks in parallel")
deliv = [
 (24, "FDE-years per new Large bank", 0.3, 0.10, "Standard: 1 month x 2 FDEs + validation support. Lean: ~1 FDE-month supervising agents"),
 (25, "FDE-years per new Mid bank", 0.25, 0.08, None),
 (26, "FDE-years per new design partner", 0.5, 0.25, "First integrations take 2–3 months; skills are still being built"),
 (27, "FDE per existing Large/design-partner bank", 0.25, 0.05, "Lean: upgrades/customization run by agents with gated review"),
 (28, "FDE per existing Mid bank", 0.15, 0.03, None),
 (29, "Minimum FDE headcount", 1.5, 1.0, None),
]
put(A, "C23", "Standard", BOLD); put(A, "D23", "Lean", BOLD); put(A, "E23", "Active", BOLD)
for r, lab, sv, lv, note in deliv:
    put(A, f"A{r}", lab)
    put(A, f"C{r}", sv, BLUE, N1, YEL if r == 24 else None, note)
    put(A, f"D{r}", lv, BLUE, N1, YEL if r == 24 else None)
    put(A, f"E{r}", f"=IF($C$3=1,C{r},D{r})", BLK, N1)
put(A, "A9", "Active volume multiplier"); put(A, "C9", "=INDEX(C6:C8,C5)", BLK, X)

put(A, "A45", "Time-series inputs", BOLD)
for i, y in enumerate(YRS):
    put(A, f"{COLS[i]}46", y, BOLD, fill=HDR)
ts = [
 (47, "New design-partner banks", [2,0,0,0,0], N1, False),
 (48, "New Large-tier banks (base)", [0,1,2,1,2], N1, True),
 (49, "New Mid-tier banks (base)", [0,1,1,2,2], N1, True),
 (50, "Outsourced-ALM partners (active, base)", [0,0,1,2,3], N1, False),
 (51, "Institutions served via partners (base)", [0,0,40,150,400], N0, True),
 (52, "Validation firms licensed (base)", [0,1,2,4,5], N1, False),
 (53, "M&A engagements (base)", [0,2,6,12,20], N0, False),
 (54, "Advisory enterprise licenses (base)", [0,0,0,1,2], N1, False),
 (55, "Core R&D / quant heads – standard", [3,4,5,7,9], N1, False),
 (56, "Sales heads – standard", [0.5,1,2,2.5,3], N1, False),
 (57, "Partner-enablement heads – standard", [0,0.5,1.5,2.5,3.5], N1, False),
 (58, "G&A heads – standard", [0.5,1,1.5,2,3], N1, False),
 (59, "Non-personnel costs, $M – standard", [0.4,0.55,0.8,1.0,1.3], M, False),
]
for r, lab, vals, f, key in ts:
    put(A, f"A{r}", lab)
    for i, v in enumerate(vals):
        put(A, f"{COLS[i]}{r}", v, BLUE, f, YEL if key else None)
lean = [
 (63, "Core R&D / quant heads – lean", [2,2.5,3,3.5,4], N1),
 (64, "Sales heads – lean", [0.5,1,1,1.5,2], N1),
 (65, "Partner-enablement heads – lean", [0,0.5,1,1,1.5], N1),
 (66, "G&A heads – lean", [0.5,0.5,1,1,1], N1),
 (67, "Non-personnel costs, $M – lean (incl. agent tokens/compute/security)", [0.5,0.7,1.0,1.3,1.6], M),
]
put(A, "A62", "Lean agent-foundry inputs (used when C3 = 2)", BOLD)
for r, lab, vals, f in lean:
    put(A, f"A{r}", lab)
    for i, v in enumerate(vals):
        put(A, f"{COLS[i]}{r}", v, BLUE, f)
put(A, "A69", "Active cost inputs (from operating model)", BOLD)
for k, lab in enumerate(["Core R&D / quant heads", "Sales heads", "Partner-enablement heads", "G&A heads", "Non-personnel costs, $M"]):
    r = 70 + k
    put(A, f"A{r}", lab)
    for c in COLS:
        put(A, f"{c}{r}", f"=IF($C$3=1,{c}{55+k},{c}{63+k})", BLK, M if k == 4 else N1)
put(A, "A76", "Example: set C3 to 1 for the standard FDE team, C5 to 1 for a slower ramp, or lower row 48–49 deal counts.", Font(name="Arial", italic=True))
A.column_dimensions["A"].width = 58
for c in "BCDEFG": A.column_dimensions[c].width = 12
A.column_dimensions["A"].width = 66

# ---------------- Model ----------------
Mo = wb.create_sheet("Model")
put(Mo, "A1", "Projections ($M unless noted) – scenario-driven", H1)
put(Mo, "A2", '="Active scenario: "&INDEX({"Downside","Base","Upside"},Assumptions!$C$5)&" | Operating model: "&INDEX({"Standard FDE team","Lean agent foundry"},Assumptions!$C$3)', BOLD)
for i, y in enumerate(YRS):
    put(Mo, f"{COLS[i]}4", y, BOLD, fill=HDR)
AS = "Assumptions!"
def row(r, label, fn, fmt=M, bold=False):
    put(Mo, f"A{r}", label, BOLD if bold else BLK)
    for i, c in enumerate(COLS):
        p = COLS[i-1] if i > 0 else None
        put(Mo, f"{c}{r}", fn(c, p), BOLD if bold else BLK, fmt)
def sec(r, t): put(Mo, f"A{r}", t, BOLD)

mult = f"{AS}$C$9"
sec(5, "Direct banks")
row(6, "New design partners", lambda c,p: f"={AS}{c}47", N1)
row(7, "New Large-tier banks", lambda c,p: f"={AS}{c}48*{mult}", N1)
row(8, "New Mid-tier banks", lambda c,p: f"={AS}{c}49*{mult}", N1)
row(9, "Cumulative design partners", lambda c,p: f"={c}6" if not p else f"={p}9+{c}6", N1)
row(10, "Cumulative Large", lambda c,p: f"={c}7" if not p else f"={p}10+{c}7", N1)
row(11, "Cumulative Mid", lambda c,p: f"={c}8" if not p else f"={p}11+{c}8", N1)
row(12, "Cumulative direct banks", lambda c,p: f"={c}9+{c}10+{c}11", N1, True)
row(13, "Penetration of regional pool", lambda c,p: f"=IFERROR({c}12/{AS}$C$41,0)", P)

sec(14, "Revenue")
row(15, "License bookings", lambda c,p: f"={c}6*{AS}$C$13+{c}7*{AS}$C$11+{c}8*{AS}$C$12")
row(16, "License revenue (cash timing)", lambda c,p: f"={AS}$C$14*{c}15" if not p else f"={AS}$C$14*{c}15+(1-{AS}$C$14)*{p}15")
row(17, "Onboarding + validation support", lambda c,p: f"={c}6*{AS}$C$17+{c}7*{AS}$C$15+{c}8*{AS}$C$16")
row(18, "Update subscription", lambda c,p: "=0" if not p else f"={p}9*{AS}$C$19+{p}10*{AS}$C$11*{AS}$C$18+{p}11*{AS}$C$12*{AS}$C$18")
row(19, "Ongoing customization services", lambda c,p: "=0" if not p else f"={p}9*{AS}$C$22+{p}10*{AS}$C$20+{p}11*{AS}$C$21")
row(20, "Direct revenue", lambda c,p: f"=SUM({c}16:{c}19)", M, True)
row(21, "ALM partner platform fees", lambda c,p: f"={AS}{c}50*{mult}*{AS}$C$31")
row(22, "Partner per-institution royalties", lambda c,p: f"={AS}{c}51*{mult}*{AS}$C$32")
row(23, "Validation-firm licenses", lambda c,p: f"={AS}{c}52*{mult}*{AS}$C$33")
row(24, "M&A engagements", lambda c,p: f"={AS}{c}53*{mult}*{AS}$C$34")
row(25, "Advisory enterprise licenses", lambda c,p: f"={AS}{c}54*{mult}*{AS}$C$35")
row(26, "Channel revenue", lambda c,p: f"=SUM({c}21:{c}25)", M, True)
row(27, "Total revenue", lambda c,p: f"={c}20+{c}26", M, True)
row(28, "Recurring revenue (subscription + annual channel licenses)", lambda c,p: f"={c}18+{c}21+{c}22+{c}23+{c}25")
row(29, "Recurring share", lambda c,p: f"=IFERROR({c}28/{c}27,0)", P)
row(30, "Channel share", lambda c,p: f"=IFERROR({c}26/{c}27,0)", P)
row(31, "Channel penetration of community pool", lambda c,p: f"=IFERROR({AS}{c}51*{mult}/{AS}$C$42,0)", P)

sec(32, "Headcount")
row(33, "FDEs", lambda c,p: (f"=MAX({AS}$E$29,{c}6*{AS}$E$26+{c}7*{AS}$E$24+{c}8*{AS}$E$25)" if not p else
    f"=MAX({AS}$E$29,{c}6*{AS}$E$26+{c}7*{AS}$E$24+{c}8*{AS}$E$25+({p}9+{p}10)*{AS}$E$27+{p}11*{AS}$E$28)"), N1)
row(34, "Core R&D / quant", lambda c,p: f"={AS}{c}70", N1)
row(35, "Sales", lambda c,p: f"={AS}{c}71", N1)
row(36, "Partner enablement", lambda c,p: f"={AS}{c}72", N1)
row(37, "G&A", lambda c,p: f"={AS}{c}73", N1)
row(38, "Total headcount", lambda c,p: f"=SUM({c}33:{c}37)", N1, True)

sec(39, "Costs and profit")
row(40, "Personnel", lambda c,p: f"={c}38*{AS}$C$37")
row(41, "Non-personnel", lambda c,p: f"={AS}{c}74")
row(42, "Commissions", lambda c,p: f"={c}15*{AS}$C$38+{c}26*{AS}$C$39")
row(43, "Total costs", lambda c,p: f"=SUM({c}40:{c}42)", M, True)
row(44, "Operating income (~EBITDA)", lambda c,p: f"={c}27-{c}43", M, True)
row(45, "Operating margin", lambda c,p: f"=IFERROR({c}44/{c}27,0)", P)
row(46, "Cumulative operating cash (pre-financing)", lambda c,p: f"={c}44" if not p else f"={p}46+{c}44")
put(Mo, "A47", "Peak funding need (excl. Phase-0 and buffer)", BOLD)
put(Mo, "C47", "=-MIN(0,MIN(C46:G46))", BOLD, M)
row(48, "New banks onboarded per FDE (capacity check)", lambda c,p: f"=IFERROR(({c}6+{c}7+{c}8)/{c}33,0)", N1)
put(Mo, "A49", "One FDE supervising agent pods handles ~4–6 concurrent onboardings (~30–50 a year); above that, hire.", Font(name="Arial", italic=True))
Mo.column_dimensions["A"].width = 58
for c in "BCDEFG": Mo.column_dimensions[c].width = 12
Mo.freeze_panes = "C5"

# ---------------- Valuation ----------------
V = wb.create_sheet("Valuation")
put(V, "A1", "Valuation – Year 5 exit and implied value today", H1)
put(V, "A2", "Multiples are judgment for private vertical financial software, not sourced comparables. Edit blue cells.", Font(name="Arial", italic=True))
for j, h in enumerate(["Low", "Mid", "High"]):
    put(V, f"{'CDE'[j]}4", h, BOLD, fill=HDR)
mults = [
 (5, "Multiple on recurring revenue", [4, 6, 8], X),
 (6, "Multiple on non-recurring revenue (license, services, M&A)", [1, 1.5, 2], X),
 (7, "Multiple on operating income (EBITDA cross-check)", [10, 15, 20], X),
]
for r, lab, vals, f in mults:
    put(V, f"A{r}", lab)
    for j, v in enumerate(vals):
        put(V, f"{'CDE'[j]}{r}", v, BLUE, f, YEL)
put(V, "A9", "Year 5 inputs (from Model)", BOLD)
put(V, "A10", "Y5 total revenue"); put(V, "C10", "=Model!G27", GRN, M)
put(V, "A11", "Y5 recurring revenue"); put(V, "C11", "=Model!G28", GRN, M)
put(V, "A12", "Y5 non-recurring revenue"); put(V, "C12", "=C10-C11", BLK, M)
put(V, "A13", "Y5 operating income"); put(V, "C13", "=Model!G44", GRN, M)
put(V, "A15", "Year 5 enterprise value ($M)", BOLD)
for j, col in enumerate("CDE"):
    put(V, f"{col}15", ["Low","Mid","High"][j], BOLD, fill=HDR)
put(V, "A16", "Revenue-mix method")
put(V, "A17", "EBITDA method")
put(V, "A18", "Blended (average)")
put(V, "A19", "Implied multiple of Y5 revenue")
for col in "CDE":
    put(V, f"{col}16", f"=$C$11*{col}5+$C$12*{col}6", BLK, M)
    put(V, f"{col}17", f"=MAX(0,$C$13)*{col}7", BLK, M)
    put(V, f"{col}18", f"=AVERAGE({col}16:{col}17)", BOLD, M)
    put(V, f"{col}19", f"=IFERROR({col}18/$C$10,0)", BLK, X)

put(V, "A21", "Implied value today (VC method)", BOLD)
put(V, "A22", "Target return multiple – pre-revenue (code + founders)"); put(V, "C22", 20, BLUE, X, YEL, "Typical seed-stage target 10–30x to compensate for failure risk")
put(V, "A23", "Target return multiple – after 2 paid design partners"); put(V, "C23", 10, BLUE, X, YEL)
put(V, "A24", "Expected dilution before exit"); put(V, "C24", 0.15, BLUE, P, None, "Low if only a seed round is raised and the company is self-funding by Y3")
for j, col in enumerate("CDE"):
    put(V, f"{col}26", ["Low","Mid","High"][j], BOLD, fill=HDR)
put(V, "A27", "Post-money today – pre-revenue")
put(V, "A28", "Post-money – after 2 paid design partners")
for col in "CDE":
    put(V, f"{col}27", f"={col}18*(1-$C$24)/$C$22", BLK, M)
    put(V, f"{col}28", f"={col}18*(1-$C$24)/$C$23", BLK, M)

put(V, "A30", "Stage ladder (judgment)", BOLD)
ladder = [
 ("Pre-revenue: working code, synthetic data, no bank users", "Value set by team, domain depth and validation evidence; code alone is cheaper to replicate with coding agents"),
 ("2 paid design partners, model-risk sign-off pending", "Re-price on proof that banks will validate and pay"),
 ("$3–5M recurring, 8–10 banks, first channel partner", "Recurring-revenue multiple starts to dominate"),
 ("Y5: $15–20M revenue, ~50% recurring, profitable", "Strategic buyers (ALM/risk vendors, core banking providers) or vertical-software PE"),
]
for k, (a, b) in enumerate(ladder):
    put(V, f"A{31+k}", a); put(V, f"C{31+k}", b)
V.column_dimensions["A"].width = 60
for c in "CDE": V.column_dimensions[c].width = 14

for ws in wb.worksheets:
    for r in ws.iter_rows():
        for c in r:
            if c.value is not None and c.font.name != "Arial":
                c.font = Font(name="Arial", bold=c.font.bold, color=c.font.color)
import os
wb.save(os.path.join(os.path.dirname(os.path.abspath(__file__)), "rates-workbench-business-model.xlsx"))
