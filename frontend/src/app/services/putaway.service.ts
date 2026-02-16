import { Injectable } from '@angular/core';
import { HttpClient, HttpParams } from '@angular/common/http';
import { Observable } from 'rxjs';
import { environment } from '../../environments/environment';

export interface PutawayItem {
  lot_no: string;
  item_key: string;
  item_description?: string;
  location_key: string;
  bin_no?: string;
  qty_received: number;
  qty_on_hand: number;
  qty_available?: number; // New field from BME - calculated available quantity
  date_received: string;
  date_expiry: string;
  exp_date?: string; // New field from BME - formatted expiry date
  uom?: string; // New field from BME - Unit of Measure
  vendor_key: string;
  vendor_lot_no: string;
  document_no: string;
  lot_status: string;
  rec_user_id: string;
}

export interface PutawayRequest {
  lot_no: string;
  item_key: string;
  from_location: string;
  to_location: string;
  bin_no: string;
  qty_to_putaway: number;
  user_id: string;
}

export interface PutawayTransactionRequest {
  lot_no: string;
  item_key: string;
  from_location: string;
  to_location: string;
  from_bin?: string;
  to_bin: string;
  qty_to_putaway: number;
  user_id: string;
}

export interface PutawayTransactionResponse {
  success: boolean;
  message: string;
  transaction_id?: number;
  doc_no?: string;
  error_details?: string;
}

export interface PutawayConfirmation {
  success: boolean;
  message: string;
  transaction_id?: number;
}

export interface ScanRequest {
  barcode: string;
  scan_type: 'Item' | 'Location' | 'Lot';
}

export interface ScanResponse {
  valid: boolean;
  scan_type: 'Item' | 'Location' | 'Lot';
  data?: ScanData;
  message: string;
}

export interface ScanData {
  Item?: {
    item_key: string;
    description: string;
    unit: string;
    uom?: string; // Unit of Measure from BME
  };
  Location?: {
    location_key: string;
    description: string;
    location_type: string;
    available_bins?: string[]; // Available bins for BME search functionality
  };
  Lot?: {
    lot_no: string;
    item_key: string;
    qty_on_hand: number;
    qty_available?: number; // Available quantity from BME
    exp_date?: string; // Expiry date from BME
    uom?: string; // Unit of Measure from BME
  };
}

export interface PutawayHistory {
  transaction_id: number;
  lot_no: string;
  item_key: string;
  from_location: string;
  to_location: string;
  bin_no: string;
  qty_moved: number;
  transaction_date: string;
  user_id: string;
}

export interface TransactionAuditEntry {
  transaction_id: number;
  record_type: string;
  document_no: string;
  description: string;
  user_id: string;
  timestamp: string;
  details?: Record<string, unknown>;
}

export interface ApiResponse<T> {
  success: boolean;
  data?: T;
  message: string;
}

export interface PaginatedLotSearchResponse {
  items: LotSearchItem[];
  total: number;
  page: number;
  pages: number;
  limit: number;
}

@Injectable({
  providedIn: 'root'
})
export class PutawayService {
  private readonly baseUrl = environment.apiUrl || 'http://localhost:4400/api';

  constructor(private http: HttpClient) { }

  /**
   * Get all pending putaway items
   */
  getPendingPutawayItems(params?: { limit?: number; status?: string }): Observable<ApiResponse<PutawayItem[]>> {
    let httpParams = new HttpParams();
    if (params?.limit) {
      httpParams = httpParams.set('limit', params.limit.toString());
    }
    if (params?.status) {
      httpParams = httpParams.set('status', params.status);
    }

    return this.http.get<ApiResponse<PutawayItem[]>>(`${this.baseUrl}/putaway/pending`, {
      params: httpParams
    });
  }

  /**
   * Scan a barcode (item, location, or lot)
   */
  scanBarcode(request: ScanRequest): Observable<ApiResponse<ScanResponse>> {
    return this.http.post<ApiResponse<ScanResponse>>(`${this.baseUrl}/putaway/scan`, request);
  }

  /**
   * Confirm putaway operation (legacy)
   */
  confirmPutaway(request: PutawayRequest): Observable<ApiResponse<PutawayConfirmation>> {
    return this.http.post<ApiResponse<PutawayConfirmation>>(`${this.baseUrl}/putaway/confirm`, request);
  }

  /**
   * Confirm putaway transaction (enhanced with audit trail)
   */
  confirmPutawayTransaction(request: PutawayTransactionRequest): Observable<ApiResponse<PutawayTransactionResponse>> {
    return this.http.post<ApiResponse<PutawayTransactionResponse>>(`${this.baseUrl}/putaway/transaction`, request);
  }

  /**
   * Get putaway history
   */
  getPutawayHistory(limit?: number): Observable<ApiResponse<PutawayHistory[]>> {
    let httpParams = new HttpParams();
    if (limit) {
      httpParams = httpParams.set('limit', limit.toString());
    }

    return this.http.get<ApiResponse<PutawayHistory[]>>(`${this.baseUrl}/putaway/history`, {
      params: httpParams
    });
  }

  /**
   * Get specific putaway item details
   */
  getPutawayItemDetails(lotNo: string, itemKey: string): Observable<ApiResponse<PutawayItem>> {
    return this.http.get<ApiResponse<PutawayItem>>(`${this.baseUrl}/putaway/item/${lotNo}/${itemKey}`);
  }

  /**
   * Get transaction audit trail
   */
  getTransactionAudit(transactionId: number): Observable<ApiResponse<TransactionAuditEntry[]>> {
    return this.http.get<ApiResponse<TransactionAuditEntry[]>>(`${this.baseUrl}/putaway/audit/${transactionId}`);
  }

  /**
   * Search for lot numbers (BME functionality)
   */
  searchLotNumbers(query: string, limit: number = 10): Observable<ApiResponse<{ lot_no: string; item_key: string; item_description?: string }[]>> {
    const params = new HttpParams()
      .set('query', query)
      .set('limit', limit.toString());

    return this.http.get<ApiResponse<{ lot_no: string; item_key: string; item_description?: string }[]>>(`${this.baseUrl}/putaway/search/lots`, { params });
  }

  /**
   * Search for available bin numbers (BME functionality)
   */
  searchBinNumbers(locationKey: string, query: string, limit: number = 10): Observable<ApiResponse<{ bin_no: string; description?: string; status?: string }[]>> {
    const params = new HttpParams()
      .set('location', locationKey)
      .set('query', query)
      .set('limit', limit.toString());

    return this.http.get<ApiResponse<{ bin_no: string; description?: string; status?: string }[]>>(`${this.baseUrl}/putaway/search/bins`, { params });
  }

  /**
   * Get available bins for a location (BME functionality)
   */
  getAvailableBins(locationKey: string): Observable<ApiResponse<{ bin_no: string; description?: string; capacity?: number; available_space?: number }[]>> {
    const params = new HttpParams().set('location', locationKey);

    return this.http.get<ApiResponse<{ bin_no: string; description?: string; capacity?: number; available_space?: number }[]>>(`${this.baseUrl}/putaway/bins/available`, { params });
  }

  // ========================================================================================
  // ENHANCED PUTAWAY API - Replicates Official App Functionality (BT-25268027 Pattern)
  // ========================================================================================

  /**
   * Search for lot details using clean API
   * Replicates official app lot search functionality
   */
  searchLot(lotNo: string): Observable<LotSearchResponse> {
    return this.http.get<LotSearchResponse>(`${this.baseUrl}/putaway/lot/${lotNo}`);
  }

  /**
   * Validate destination bin using clean API
   * Replicates official app bin validation functionality
   */
  validateBin(location: string, binNo: string): Observable<BinValidationResponse> {
    return this.http.get<BinValidationResponse>(`${this.baseUrl}/putaway/bin/${location}/${binNo}`);
  }

  /**
   * Execute bin transfer using clean API
   * Replicates official app BT-25268027 transaction pattern
   */
  executeBinTransfer(request: BinTransferRequest): Observable<TransactionResponse> {
    return this.http.post<TransactionResponse>(`${this.baseUrl}/putaway/transfer`, request);
  }

  /**
   * Search multiple lots with optional query filter
   * Replicates BME lot selection dialog functionality
   */
  searchMultipleLots(query?: string, limit: number = 20): Observable<LotSearchItem[]> {
    let params = new HttpParams();
    if (query) {
      params = params.set('query', query);
    }
    params = params.set('limit', limit.toString());

    return this.http.get<LotSearchItem[]>(`${this.baseUrl}/putaway/lots/search`, { params });
  }

  /**
   * Search multiple lots with pagination support
   * Enhanced version with server-side pagination
   */
  searchMultipleLotsWithPagination(query?: string, page: number = 1, limit: number = 20): Observable<PaginatedLotSearchResponse> {
    let params = new HttpParams();
    if (query) {
      params = params.set('query', query);
    }
    params = params.set('page', page.toString());
    params = params.set('limit', limit.toString());

    return this.http.get<PaginatedLotSearchResponse>(`${this.baseUrl}/putaway/lots/search`, { params });
  }

  /**
   * Search for bins with pagination support
   * Enhanced version with server-side pagination
   *
   * When lotContext is provided, the search will show if each bin contains
   * the specified lot and what status it has (helps users see consolidation targets)
   */
  searchBinsWithPagination(
    query?: string,
    page: number = 1,
    limit: number = 20,
    lotContext?: { lot_no: string; item_key: string; location: string }
  ): Observable<PaginatedBinSearchResponse> {
    let params = new HttpParams();
    if (query) {
      params = params.set('query', query);
    }
    params = params.set('page', page.toString());
    params = params.set('limit', limit.toString());

    // Add lot context parameters if provided
    if (lotContext) {
      params = params.set('lot_no', lotContext.lot_no);
      params = params.set('item_key', lotContext.item_key);
      params = params.set('location', lotContext.location);
    }

    return this.http.get<PaginatedBinSearchResponse>(`${this.baseUrl}/putaway/bins/search`, { params });
  }

  /**
   * Get putaway service health status
   */
  getPutawayHealth(): Observable<HealthResponse> {
    return this.http.get<HealthResponse>(`${this.baseUrl}/putaway/health`);
  }

  /**
   * Get all active putaway remarks for dropdown
   */
  getActiveRemarks(): Observable<{ success: boolean; data: { id: number; remark_name: string }[] }> {
    return this.http.get<{ success: boolean; data: { id: number; remark_name: string }[] }>(`${this.baseUrl}/putaway/remarks`);
  }
  /**
   * Search for lot transactions
   */
  searchLotTransactions(lotNo: string, binNo: string): Observable<LotTransactionItem[]> {
    return this.http.get<LotTransactionItem[]>(`${this.baseUrl}/putaway/transactions/${lotNo}/${binNo}`);
  }

  /**
   * Transfer committed items
   */
  transferCommittedTransactions(request: CommittedTransferRequest): Observable<CommittedTransferResult> {
    return this.http.post<CommittedTransferResult>(`${this.baseUrl}/putaway/transfer/committed`, request);
  }
}

// ========================================================================================
// CLEAN PUTAWAY API INTERFACES - Official App Compatibility
// ========================================================================================

export interface LotSearchResponse {
  lot_no: string;
  item_key: string;
  location: string;
  current_bin: string;
  qty_on_hand: number;
  qty_commit_sales: number;
  qty_available: number;
  expiry_date?: string;
  item_description: string;
  uom: string;
  lot_status: string;
}

export interface BinValidationResponse {
  bin_no: string;
  location: string;
  is_valid: boolean;
  message: string;
}

export interface BinTransferRequest {
  lot_no: string;
  item_key: string;
  location: string;
  bin_from: string;
  bin_to: string;
  transfer_qty: number;
  user_id: string;
  remarks?: string;
  referenced?: string;
}

export interface TransactionResponse {
  success: boolean;
  document_no: string;
  message: string;
  timestamp: string;
  source_lot_status?: string;
  destination_lot_status?: string;
}

export interface HealthResponse {
  status: string;
  service: string;
  timestamp: string;
  version: string;
}

export interface LotSearchItem {
  lot_no: string;
  item_key: string;
  item_description: string;
  location: string;
  current_bin: string;
  qty_on_hand: number;
  qty_commit_sales: number;
  qty_available: number;
  date_received?: string;
  expiry_date?: string;
  uom: string;
  lot_status: string;
}

export interface BinSearchItem {
  bin_no: string;
  location: string;
  description: string;
  aisle: string;
  row: string;
  rack: string;
  lot_status?: string; // Optional: lot status if this bin contains the specified lot
}

export interface PaginatedBinSearchResponse {
  items: BinSearchItem[];
  total: number;
  page: number;
  pages: number;
  limit: number;
}

export interface LotTransactionItem {
  lot_tran_no: number;
  lot_no: string;
  bin_no: string;
  doc_no: string;
  issue_doc_line_no: number | null;
  qty: number;
  transaction_type: number;
  tran_typ: string;  // Human readable transaction type name
  transaction_date: string;
  status: string;
}

export interface CommittedTransferRequest {
  lot_no: string;
  item_key: string;
  location: string;
  source_bin: string;
  to_bin: string;
  transfer_qty: number;
  user_id: string;
  remarks?: string;
  referenced?: string;
}

export interface CommittedTransferResult {
  success: boolean;
  message: string;
  document_no: string;
  timestamp: string;
}

// Inside PutawayService class:
// Note: Inserting methods into the class using a separate edit since replacing the whole file is expensive.
// I will append interfaces here and then methods inside the class in next tool call or same if I can target correctly.
// Since `replace_file_content` targetting line 415-421 is at the end of file (interfaces), I can add interfaces.
// But I also need to add methods to the class.
// I'll add interfaces at the end first.