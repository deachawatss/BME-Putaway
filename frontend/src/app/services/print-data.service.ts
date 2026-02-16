import { Injectable } from '@angular/core';
import { HttpClient } from '@angular/common/http';
import { Observable } from 'rxjs';
import { map } from 'rxjs/operators';
import { BulkRun, BulkPickedItem } from './bulk-runs.service';
import { AuthService } from './auth.service';

// Local API Response interface for this service
interface ApiResponse<T> {
  success: boolean;
  data?: T;
  message: string;
}

// Enhanced interface for print functionality that includes all needed fields
interface EnhancedBulkPickedItem extends BulkPickedItem {
  batch_no?: string;
  lot_no?: string;
  bin_no?: string;
  unit?: string;
  picked_qty?: number;
  modified_by?: string;
  modified_date?: string;
  rec_user_id?: string;
}
import { ConfigService } from './config.service';

export interface LotPickingDetail {
  run_no: number;
  batch_no: string;
  line_id: number;
  item_key: string;
  lot_no: string;
  bin_no: string;
  qty_received: number;
  pack_size: number;
  rec_userid: string;
  modified_by: string;
  rec_date: string;
  picked_bulk_qty: number;
  picked_qty: number;
}

export interface PrintLabelData {
  runData: BulkRun;
  pickedItems: EnhancedBulkPickedItem[];
  lotDetails: LotPickingDetail[];
  completedBatches: string[];
}


@Injectable({
  providedIn: 'root'
})
export class PrintDataService {

  private apiUrl: string;

  constructor(
    private http: HttpClient,
    private configService: ConfigService,
    private authService: AuthService
  ) {
    this.apiUrl = this.configService.getApiUrl();
  }

  /**
   * Get complete print label data for a run
   */
  getPrintLabelData(runNo: number): Observable<PrintLabelData> {
    return this.http.get<{
      run_data: BulkRun;
      picked_items: BulkPickedItem[];
      lot_details: LotPickingDetail[];
    }>(`${this.apiUrl}/runs/${runNo}/print-data`).pipe(
      map(response => {
        const completedBatches = this.getCompletedBatches(response.picked_items);
        
        return {
          runData: response.run_data,
          pickedItems: this.enhancePickedItemsWithLotData(response.picked_items, response.lot_details),
          lotDetails: response.lot_details,
          completedBatches
        };
      })
    );
  }

  /**
   * Get lot picking details for a specific run
   */
  getLotPickingDetails(runNo: number): Observable<LotPickingDetail[]> {
    return this.http.get<ApiResponse<LotPickingDetail[]>>(`${this.apiUrl}/bulk-runs/${runNo}/lot-details`)
      .pipe(map(response => response.data ?? []));
  }

  /**
   * Enhance picked items with lot number and bin location data
   */
  private enhancePickedItemsWithLotData(
    pickedItems: BulkPickedItem[], 
    lotDetails: LotPickingDetail[]
  ): EnhancedBulkPickedItem[] {
    return pickedItems.map(item => {
      // Find matching lot detail for this item
      const lotDetail = lotDetails.find(lot => 
        lot.run_no === item.run_no &&
        lot.batch_no === item.batch_no &&
        lot.line_id === item.line_id &&
        lot.item_key === item.item_key
      );

      return {
        ...item,
        lot_no: lotDetail?.lot_no || '',
        bin_no: lotDetail?.bin_no || ''
      };
    });
  }

  /**
   * Get list of completed batches (batches with at least one picked item)
   */
  getCompletedBatches(pickedItems: EnhancedBulkPickedItem[]): string[] {
    const batchesWithPicking = new Set<string>();
    
    pickedItems.forEach(item => {
      if (item.picked_bulk_qty !== null && 
          item.picked_bulk_qty !== undefined && 
          this.parseQuantity(item.picked_bulk_qty) > 0) {
        batchesWithPicking.add(item.batch_no || '');
      }
    });
    
    return Array.from(batchesWithPicking).sort();
  }

  /**
   * Check if a batch is completed (all required ingredients picked)
   */
  isBatchCompleted(batchNo: string, pickedItems: EnhancedBulkPickedItem[]): boolean {
    const batchItems = pickedItems.filter(item => item.batch_no === batchNo);
    const requiredItems = batchItems.filter(item => 
      item.to_picked_bulk_qty !== null && 
      this.parseQuantity(item.to_picked_bulk_qty) > 0
    );
    const pickedItems_filtered = batchItems.filter(item => 
      item.picked_bulk_qty !== null && 
      this.parseQuantity(item.picked_bulk_qty) > 0
    );
    
    return requiredItems.length > 0 && requiredItems.length === pickedItems_filtered.length;
  }

  /**
   * Get count of completed batches for display
   */
  getCompletedBatchCount(pickedItems: EnhancedBulkPickedItem[]): number {
    return this.getCompletedBatches(pickedItems).length;
  }

  /**
   * Group picked items by batch number
   */
  groupItemsByBatch(pickedItems: EnhancedBulkPickedItem[]): { [batchNo: string]: EnhancedBulkPickedItem[] } {
    return pickedItems.reduce((groups, item) => {
      const batchNo = item.batch_no || '';
      if (!groups[batchNo]) {
        groups[batchNo] = [];
      }
      groups[batchNo].push(item);
      return groups;
    }, {} as { [batchNo: string]: EnhancedBulkPickedItem[] });
  }

  /**
   * Validate if print data is ready
   */
  validatePrintData(runData: BulkRun, pickedItems: EnhancedBulkPickedItem[]): {
    isValid: boolean;
    errors: string[];
  } {
    const errors: string[] = [];

    if (!runData) {
      errors.push('Run data is missing');
    }

    if (!pickedItems || pickedItems.length === 0) {
      errors.push('No picked items found');
    }

    const completedBatches = this.getCompletedBatches(pickedItems);
    if (completedBatches.length === 0) {
      errors.push('No completed batches found');
    }

    return {
      isValid: errors.length === 0,
      errors
    };
  }

  /**
   * Format date for Bangkok timezone
   */
  formatBangkokDate(date: Date = new Date()): string {
    return new Intl.DateTimeFormat('en-GB', {
      timeZone: 'Asia/Bangkok',
      day: '2-digit',
      month: '2-digit',
      year: '2-digit'
    }).format(date);
  }

  /**
   * Format datetime for Bangkok timezone
   */
  formatBangkokDateTime(date: Date = new Date()): string {
    return new Intl.DateTimeFormat('en-GB', {
      timeZone: 'Asia/Bangkok',
      day: '2-digit',
      month: '2-digit',
      year: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: true
    }).format(date);
  }

  /**
   * Get the most recent user who modified items in a batch
   */
  getMostRecentUserForBatch(batchNo: string, pickedItems: EnhancedBulkPickedItem[]): string {
    const batchItems = pickedItems.filter(item => 
      item.batch_no === batchNo && 
      item.picked_bulk_qty !== null
    );

    if (batchItems.length === 0) {
      return 'SYSTEM';
    }

    // Find the most recently modified item
    const mostRecent = batchItems.reduce((latest, item) => {
      const itemDate = new Date(item.picking_date || item.modified_date || '1970-01-01');
      const latestDate = new Date(latest.picking_date || latest.modified_date || '1970-01-01');
      return itemDate > latestDate ? item : latest;
    });

    return mostRecent.modified_by || mostRecent.rec_user_id || 'SYSTEM';
  }

  /**
   * Calculate page number for a batch in the run
   */
  calculatePageNumber(batchNo: string, runData: BulkRun, completedBatches: string[]): string {
    const batchIndex = completedBatches.indexOf(batchNo) + 1;
    const totalPages = runData.no_of_batches;
    return `Page ${batchIndex} of ${totalPages}`;
  }

  /**
   * Parse quantity string to number (handles string and number types)
   */
  private parseQuantity(quantity: string | number | undefined | null): number {
    if (quantity === null || quantity === undefined) return 0;
    if (typeof quantity === 'number') return quantity;
    if (typeof quantity === 'string') {
      const parsed = parseFloat(quantity);
      return isNaN(parsed) ? 0 : parsed;
    }
    return 0;
  }

  /**
   * Print labels directly to system print without modal dialog
   */
  printLabelsDirectly(runNo: number): Observable<void> {
    return this.getPrintLabelData(runNo).pipe(
      map(printData => {
        this.printLabelsWithHTML(printData);
      })
    );
  }

  /**
   * Print labels using existing component data (no API call needed)
   */
  printLabelsFromComponentData(runData: any, pickedItems: any[], completedBatches: string[]): void {
    
    // Fetch lot details and then print
    this.getLotPickingDetails(runData.run_no).subscribe({
      next: (lotDetails) => {
        const printData: PrintLabelData = {
          runData: runData,
          pickedItems: pickedItems,
          lotDetails: lotDetails,
          completedBatches: completedBatches
        };
        
        this.printLabelsWithHTML(printData);
      },
      error: (error) => {
        console.warn('Could not fetch lot details, printing without them:', error);
        // Fallback: print without lot details
        const printData: PrintLabelData = {
          runData: runData,
          pickedItems: pickedItems,
          lotDetails: [],
          completedBatches: completedBatches
        };
        
        this.printLabelsWithHTML(printData);
      }
    });
  }

  /**
   * Generate HTML and trigger direct system print
   */
  private printLabelsWithHTML(printData: PrintLabelData): void {
    if (!printData.runData || printData.completedBatches.length === 0) {
      console.error('No print data available');
      return;
    }

    // Generate HTML for all labels
    const htmlContent = this.generatePrintHTML(printData);
    
    // Create temporary iframe for printing
    const printFrame = document.createElement('iframe');
    printFrame.style.position = 'absolute';
    printFrame.style.left = '-9999px';
    printFrame.style.width = '0px';
    printFrame.style.height = '0px';
    
    document.body.appendChild(printFrame);
    
    const frameDoc = printFrame.contentWindow?.document;
    if (frameDoc) {
      frameDoc.open();
      frameDoc.write(htmlContent);
      frameDoc.close();
      
      // Wait for content to load then print
      setTimeout(() => {
        printFrame.contentWindow?.print();
        
        // Clean up iframe after printing
        setTimeout(() => {
          document.body.removeChild(printFrame);
        }, 1000);
      }, 500);
    }
  }

  /**
   * Generate complete HTML for direct printing
   */
  private generatePrintHTML(printData: PrintLabelData): string {
    const labels = this.generateLabelsFromPrintData(printData);
    
    return `
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Bulk Picking Labels</title>
    <style>
        ${this.getPrintCSS()}
    </style>
</head>
<body>
    <div class="labels-container">
        ${labels.map(label => this.generateLabelHTML(label)).join('')}
    </div>
</body>
</html>`;
  }

  /**
   * Generate labels from print data
   */
  private generateLabelsFromPrintData(printData: PrintLabelData): any[] {
    const batchGroups = this.groupItemsByBatch(printData.pickedItems);
    
    return printData.completedBatches.map(batchNo => {
      const batchItems = batchGroups[batchNo] || [];
      return this.createLabelData(printData.runData, batchNo, batchItems, printData.completedBatches, printData.lotDetails);
    });
  }

  /**
   * Create label data structure
   */
  private createLabelData(runData: any, batchNo: string, items: EnhancedBulkPickedItem[], completedBatches: string[], lotDetails: LotPickingDetail[]): any {
    const currentDate = new Date();
    const bangkokTime = this.formatBangkokDate(currentDate);
    const bangkokDateTime = this.formatBangkokDateTime(currentDate);
    
    const batchIndex = completedBatches.indexOf(batchNo) + 1;
    const totalPages = runData.no_of_batches;
    
    // Get current logged-in user instead of hard-coded fallback
    const currentUser = this.authService.getCurrentUser();
    const pickedByUser = currentUser?.username || this.getMostRecentUserForBatch(batchNo, items) || 'SYSTEM';
    
    // Create print items directly from lot details (one line per lot/bin combination)
    const batchLotDetails = lotDetails.filter(lot => lot.batch_no === batchNo);

    const enhancedItems = batchLotDetails.map(lotDetail => {
      // Find corresponding item data for unit and pack size
      const item = items.find(i => i.item_key === lotDetail.item_key);

      // Calculate bag count from quantity/pack size for each lot
      const bags = lotDetail.qty_received && lotDetail.pack_size > 0
        ? Math.ceil(lotDetail.qty_received / lotDetail.pack_size)
        : Math.ceil(this.parseQuantity(lotDetail.picked_bulk_qty));

      return {
        itemNo: lotDetail.item_key,
        lotNo: lotDetail.lot_no,
        binNo: lotDetail.bin_no,
        bagQuantity: bags,
        totalQuantity: this.parseQuantity(lotDetail.qty_received),
        unit: item?.unit || 'KG',
        packSize: this.parseQuantity(lotDetail.pack_size),
        checkbox: false
      };
    });
    
    return {
      header: {
        title: 'BULK SUMMARY'
        // Remove date and description from header - should only show title
      },
      productInfo: {
        productKey: runData.formula_id,
        description: runData.formula_desc || '',
        runNo: runData.run_no,
        batchNo: batchNo,
        date: bangkokTime,
        pageInfo: `Page ${batchIndex} of ${totalPages}`
      },
      items: enhancedItems,
      footer: {
        pickedBy: pickedByUser,
        verificationLine: 'Verified by: _______________',
        manualDate: 'Date: ___/___/___',
        datePrinted: `Date Printed: ${bangkokDateTime}`
      }
    };
  }

  /**
   * Generate HTML for a single label
   */
  private generateLabelHTML(label: any): string {
    const itemsHTML = label.items.map((item: any) => `
      <tr class="item-row">
        <td class="item-no">${item.itemNo}</td>
        <td class="lot-info">
          <div class="lot-no">${item.lotNo}</div>
          <div class="bin-no">${item.binNo}</div>
        </td>
        <td class="bag-qty">
          <div class="bag-number">${item.bagQuantity}</div>
          <div class="checkbox-container">
            <span class="checkbox">☐</span>
          </div>
        </td>
        <td class="qty-info">
          <div class="total-qty">${this.parseQuantity(item.totalQuantity).toFixed(1)}</div>
          <div class="pack-size">${this.parseQuantity(item.packSize).toFixed(2)}</div>
        </td>
        <td class="unit">${item.unit}</td>
      </tr>
    `).join('');

    return `
    <div class="label-page">
      <!-- Label Header - Only BULK SUMMARY -->
      <div class="label-header">
        <div class="label-title">${label.header.title}</div>
        <div class="header-divider"></div>
      </div>
      
      <!-- Product Information -->
      <div class="product-info">
        <div class="product-line">
          <span class="product-label">Product:</span>
          <span class="product-key">${label.productInfo.productKey}</span>
          <span class="product-desc">${label.productInfo.description}</span>
        </div>
        <div class="run-info-line">
          <span class="info-label">Run:</span>
          <span class="info-value">${label.productInfo.runNo}</span>
          <span class="date-label">Date:</span>
          <span class="date-value">${label.productInfo.date}</span>
        </div>
        <div class="batch-info-line">
          <div class="batch-group">
            <span class="batch-label">Batch:</span>
            <span class="batch-value">${label.productInfo.batchNo}</span>
          </div>
          <span class="page-value">${label.productInfo.pageInfo}</span>
        </div>
        <div class="product-divider"></div>
      </div>
      
      <!-- Items Table -->
      <div class="items-section">
        <table class="items-table">
          <thead>
            <tr>
              <th class="col-item">Item No.</th>
              <th class="col-lot">Lot-No</th>
              <th class="col-bag">Bag</th>
              <th class="col-qty">QTY</th>
              <th class="col-um">UM</th>
            </tr>
          </thead>
          <tbody>
            ${itemsHTML}
          </tbody>
        </table>
      </div>
      
      <!-- Footer -->
      <div class="label-footer">
        <div class="footer-row">
          <div class="footer-left">
            <div class="picked-by">
              <span class="footer-label">Picked/Printed by:</span>
              <span class="footer-value">${label.footer.pickedBy}</span>
            </div>
            <div class="date-printed">${label.footer.datePrinted}</div>
          </div>
          <div class="footer-right">
            <div class="verified-by">${label.footer.verificationLine}</div>
            <div class="manual-date">${label.footer.manualDate}</div>
          </div>
        </div>
      </div>
    </div>`;
  }

  /**
   * Get CSS styles for direct printing
   */
  private getPrintCSS(): string {
    return `
      /* Reset all margins and padding */
      * {
        margin: 0;
        padding: 0;
        box-sizing: border-box;
      }
      
      body {
        margin: 0;
        padding: 0;
        font-family: Arial, sans-serif;
      }
      
      .labels-container {
        display: block;
        padding: 0;
        margin: 0;
        background: none;
      }
      
      /* Each label takes exactly one page - 4x4 inch */
      .label-page {
        width: 4in;
        height: 4in;
        page-break-after: always;
        page-break-inside: avoid;
        margin: 0;
        padding: 8pt;
        border: 1pt solid #000;
        background: white;
        font-family: Arial, sans-serif;
        font-size: 8pt;
        position: relative;
        display: flex;
        flex-direction: column;
      }
      
      /* Remove page break after last label */
      .label-page:last-child {
        page-break-after: avoid;
      }
      
      /* Label Header Styles */
      .label-header {
        text-align: center;
        margin-bottom: 6pt;
      }
      
      .label-title {
        font-weight: bold;
        font-size: 10pt;
        margin-bottom: 2pt;
      }
      
      .label-product-type {
        font-weight: bold;
        font-size: 9pt;
        margin-bottom: 2pt;
      }
      
      .label-date {
        font-size: 8pt;
        margin-bottom: 4pt;
      }
      
      .header-divider {
        border-bottom: 2px solid #000;
        margin-bottom: 6pt;
      }
      
      /* Product Information Styles */
      .product-info {
        margin-bottom: 6pt;
        font-size: 8pt;
      }
      
      .product-line {
        margin-bottom: 2pt;
      }
      
      .product-label, .info-label, .date-label, .batch-label {
        font-weight: bold;
      }
      
      .product-key {
        margin-left: 4pt;
        margin-right: 8pt;
      }
      
      .run-info-line {
        margin-bottom: 2pt;
      }
      
      .batch-info-line {
        display: flex;
        justify-content: space-between;
        margin-bottom: 2pt;
      }
      
      .batch-group {
        display: flex;
        gap: 2pt;
      }
      
      .page-value {
        text-align: right;
      }
      
      .info-label, .date-label, .batch-label {
        margin-right: 4pt;
      }
      
      .date-label {
        margin-left: 16pt;
      }
      
      .product-divider {
        border-bottom: 1px solid #000;
        margin-bottom: 6pt;
      }
      
      /* Items Table Styles */
      .items-section {
        flex: 1;
        margin-bottom: 6pt;
      }
      
      .items-table {
        width: 100%;
        border-collapse: collapse;
        font-size: 7pt;
        border: 1pt solid #000;
      }
      
      .items-table th {
        border: 1pt solid #000;
        padding: 1pt 2pt;
        font-weight: bold;
        text-align: center;
        background-color: #f0f0f0;
        font-size: 7pt;
      }
      
      .items-table td {
        border: 1pt solid #000;
        padding: 1pt 2pt;
        vertical-align: top;
        font-size: 7pt;
      }
      
      /* Column widths */
      .col-item { width: 20%; }
      .col-lot { width: 25%; }
      .col-bag { width: 15%; }
      .col-qty { width: 20%; }
      .col-um { width: 10%; }
      
      /* Item row styles */
      .item-no {
        font-weight: bold;
        text-align: left;
      }
      
      .lot-info {
        text-align: left;
      }
      
      .lot-no {
        font-weight: bold;
        margin-bottom: 1pt;
      }
      
      .bin-no {
        font-size: 6pt;
        color: #666;
      }
      
      .bag-qty {
        text-align: center;
      }
      
      .checkbox-container {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: 2pt;
      }
      
      .checkbox {
        font-size: 8pt;
        border: 1pt solid #000;
        width: 8pt;
        height: 8pt;
        display: inline-block;
        text-align: center;
        line-height: 6pt;
      }
      
      .bag-number {
        font-weight: bold;
      }
      
      .qty-info {
        text-align: right;
      }
      
      .total-qty {
        font-weight: bold;
        margin-bottom: 1pt;
      }
      
      .pack-size {
        font-size: 5pt;
        color: #666;
      }
      
      .unit {
        text-align: center;
        font-weight: bold;
      }
      
      /* Footer Styles */
      .label-footer {
        margin-top: auto;
        padding-top: 4pt;
        border-top: 1pt solid #000;
        font-size: 7pt;
      }
      
      .footer-row {
        display: flex;
        justify-content: space-between;
        margin-bottom: 2pt;
      }
      
      .footer-left {
        display: flex;
        flex-direction: column;
      }
      
      .footer-right {
        display: flex;
        align-items: center;
      }
      
      .picked-by {
        margin-bottom: 2pt;
      }
      
      .footer-label {
        font-weight: bold;
      }
      
      .footer-value {
        margin-left: 4pt;
      }
      
      .manual-date {
        font-weight: bold;
        margin-bottom: 2pt;
      }
      
      .verified-by {
        font-weight: bold;
      }
      
      .date-printed {
        font-size: 6pt;
        text-align: center;
      }
      
      @media print {
        @page {
          size: 4in 4in;
          margin: 0;
        }
        
        body {
          margin: 0;
          padding: 0;
        }
      }
    `;
  }
}