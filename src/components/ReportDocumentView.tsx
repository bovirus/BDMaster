/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import {
  Accordion,
  AccordionDetails,
  AccordionSummary,
  Box,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  Typography,
} from "@mui/material";
import ExpandMoreIcon from "@mui/icons-material/ExpandMore";
import { useTranslation } from "react-i18next";
import type { ReportDocument } from "../lib/report";

export default function ReportDocumentView({ document }: { document: ReportDocument }) {
  const { t } = useTranslation();

  return (
    <Box sx={{ display: "flex", flexDirection: "column", gap: 1 }}>
      {document.sections.map((section, sectionIndex) => (
        <Accordion key={`${section.title}-${sectionIndex}`} defaultExpanded disableGutters>
          <AccordionSummary expandIcon={<ExpandMoreIcon />}>
            <Typography variant="subtitle2" sx={{ fontWeight: 700 }}>
              {section.title}
            </Typography>
          </AccordionSummary>
          <AccordionDetails sx={{ pt: 0 }}>
            {section.tables.map((table, tableIndex) => (
              <Box key={`${table.title ?? "table"}-${tableIndex}`} sx={{ mb: 2 }}>
                {table.title && (
                  <Typography variant="subtitle2" sx={{ mb: 0.75, fontWeight: 700 }}>
                    {table.title}
                  </Typography>
                )}
                <TableContainer sx={{ overflowX: "auto" }}>
                  <Table size="small">
                    <TableHead>
                      <TableRow>
                        {table.headers.map((header) => (
                          <TableCell key={header} sx={{ fontWeight: 700, whiteSpace: "nowrap" }}>
                            {header}
                          </TableCell>
                        ))}
                      </TableRow>
                    </TableHead>
                    <TableBody>
                      {table.rows.map((row, rowIndex) => (
                        <TableRow key={rowIndex} hover>
                          {row.map((cell, cellIndex) => (
                            <TableCell
                              key={cellIndex}
                              align={cell.align === "right" ? "right" : "left"}
                              sx={{ whiteSpace: cell.align === "right" ? "nowrap" : "normal" }}
                            >
                              {cell.value || t("report.emptyValue")}
                            </TableCell>
                          ))}
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                </TableContainer>
              </Box>
            ))}
          </AccordionDetails>
        </Accordion>
      ))}
    </Box>
  );
}
