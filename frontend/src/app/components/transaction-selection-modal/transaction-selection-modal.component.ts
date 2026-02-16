import { Component, EventEmitter, Input, Output, signal, effect, inject, input, computed } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { PutawayService, LotTransactionItem } from '../../services/putaway.service';

@Component({
  selector: 'app-transaction-selection-modal',
  standalone: true,
  imports: [CommonModule, FormsModule],
  template: `
    <!-- Modal Overlay -->
    <div 
      *ngIf="isOpen()" 
      class="tw-fixed tw-inset-0 tw-z-50 tw-flex tw-items-center tw-justify-center tw-bg-black/50 tw-p-4"
      (click)="onOverlayClick($event)">
      
      <!-- Modal Dialog -->
      <div 
        class="tw-bg-white tw-rounded-lg tw-shadow-xl tw-w-full tw-max-w-sm sm:tw-max-w-md md:tw-max-w-3xl lg:tw-max-w-4xl tw-max-h-[90vh] tw-mx-2 sm:tw-mx-4 tw-flex tw-flex-col"
        (click)="$event.stopPropagation()">
        
        <!-- Modal Header -->
        <div class="nwfth-button-primary tw-p-4 tw-rounded-t-lg">
          <div class="tw-flex tw-items-center tw-justify-between">
            <h2 class="tw-text-xl tw-font-bold tw-text-white tw-flex tw-items-center tw-gap-3">
              <span class="tw-text-2xl"></span>
              <span>Select Transactions for Lot: {{ lotNo() }} / Bin: {{ binNo() }}</span>
            </h2>
            <button 
              type="button"
              (click)="onClose()"
              class="tw-bg-white/20 hover:tw-bg-white/30 tw-text-white tw-p-2 tw-rounded-lg tw-transition-all tw-duration-200 hover:tw-scale-105"
              aria-label="Close dialog">
              ✕
            </button>
          </div>
          <!-- Target Quantity Info -->
          <div *ngIf="targetQty() > 0" class="tw-mt-2 tw-text-white/90 tw-text-sm">
            Target Quantity: <span class="tw-font-bold">{{ targetQty().toFixed(2) }}</span>
          </div>
        </div>

        <!-- Results Section -->
        <div class="tw-flex-1 tw-overflow-auto tw-p-4">
          <!-- Loading State -->
          <div *ngIf="isLoading()" class="tw-p-8 tw-text-center">
            <div class="tw-w-8 tw-h-8 tw-border-4 tw-border-amber-500 tw-border-t-transparent tw-rounded-full tw-animate-spin tw-mx-auto tw-mb-4"></div>
            <p class="tw-text-gray-600">Loading transactions...</p>
          </div>

          <!-- Empty State -->
          <div *ngIf="!isLoading() && transactions().length === 0" class="tw-p-8 tw-text-center">
            <span class="tw-text-6xl tw-text-gray-300 tw-block tw-mb-4">📭</span>
            <p class="tw-text-gray-600 tw-mb-2">No transactions found for this lot.</p>
          </div>

          <!-- Transactions Table -->
          <div *ngIf="!isLoading() && transactions().length > 0" class="tw-overflow-x-auto">
            <table class="tw-w-full tw-text-sm tw-table-auto tw-border-collapse">
              <thead class="tw-bg-gray-50 tw-border-b tw-border-gray-200 tw-sticky tw-top-0">
                <tr>
                  <th class="tw-px-3 tw-py-3 tw-text-center tw-min-w-[40px]">
                    <input type="checkbox" [checked]="areAllSelected()" (change)="toggleAll($event)" class="tw-rounded tw-border-gray-300 tw-text-amber-600 focus:tw-ring-amber-500">
                  </th>
                  <th class="tw-px-3 tw-py-3 tw-text-left tw-font-semibold tw-text-gray-700">Lot No</th>
                  <th class="tw-px-3 tw-py-3 tw-text-center tw-font-semibold tw-text-gray-700">Bin No</th>
                  <th class="tw-px-3 tw-py-3 tw-text-left tw-font-semibold tw-text-gray-700">Doc. No</th>
                  <th class="tw-px-3 tw-py-3 tw-text-center tw-font-semibold tw-text-gray-700">Line No</th>
                  <th class="tw-px-3 tw-py-3 tw-text-right tw-font-semibold tw-text-gray-700">Quantity</th>
                  <th class="tw-px-3 tw-py-3 tw-text-left tw-font-semibold tw-text-gray-700">Transaction Type</th>
                </tr>
              </thead>
              <tbody class="tw-divide-y tw-divide-gray-200">
                <tr *ngFor="let item of transactions(); trackBy: trackByTranNo" 
                    class="hover:tw-bg-gray-50 tw-transition-colors tw-duration-150"
                    [class.tw-bg-amber-50]="isSelected(item.lot_tran_no)">
                  <td class="tw-px-3 tw-py-3 tw-text-center">
                    <input type="checkbox" 
                           [checked]="isSelected(item.lot_tran_no)" 
                           (change)="toggleSelection(item.lot_tran_no)"
                           class="tw-rounded tw-border-gray-300 tw-text-amber-600 focus:tw-ring-amber-500">
                  </td>
                  <td class="tw-px-3 tw-py-3 tw-text-gray-900 tw-font-medium">{{ item.lot_no }}</td>
                  <td class="tw-px-3 tw-py-3 tw-text-center tw-text-gray-700">{{ item.bin_no }}</td>
                  <td class="tw-px-3 tw-py-3 tw-text-gray-900">{{ item.doc_no || '-' }}</td>
                  <td class="tw-px-3 tw-py-3 tw-text-center tw-text-gray-700">{{ item.issue_doc_line_no || '-' }}</td>
                  <td class="tw-px-3 tw-py-3 tw-text-right tw-text-gray-900 tw-font-bold">{{ item.qty.toFixed(2) }}</td>
                  <td class="tw-px-3 tw-py-3 tw-text-gray-700">{{ item.tran_typ }}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>

        <!-- Quantity Mismatch Warning -->
        <div *ngIf="showQtyMismatchWarning()" class="tw-px-4 tw-py-2 tw-bg-yellow-50 tw-text-yellow-800 tw-text-sm tw-border-t tw-border-yellow-200">
          <span class="tw-font-bold">⚠️ Quantity Mismatch:</span> 
          Selected {{ selectedTotalQty().toFixed(2) }} but target is {{ targetQty().toFixed(2) }}. 
          Please select transactions totaling exactly {{ targetQty().toFixed(2) }}.
        </div>

        <!-- Error Message -->
        <div *ngIf="errorMessage()" class="tw-px-4 tw-py-2 tw-bg-red-50 tw-text-red-700 tw-text-sm tw-border-t tw-border-red-100">
          {{ errorMessage() }}
        </div>

        <!-- Modal Footer -->
        <div class="tw-p-4 tw-border-t tw-border-gray-200 tw-bg-gray-50 tw-rounded-b-lg">
          <div class="tw-flex tw-justify-between tw-items-center">
            <div class="tw-text-sm tw-text-gray-600">
              <div>
                <span class="tw-font-bold" [class.tw-text-amber-700]="!isQtyMatched()" [class.tw-text-green-700]="isQtyMatched()">
                  {{ selectedTotalQty().toFixed(2) }}
                </span> 
                / 
                <span class="tw-font-bold">{{ targetQty().toFixed(2) }}</span>
                <span class="tw-text-gray-500 tw-ml-1">selected</span>
              </div>
              <div *ngIf="isQtyMatched()" class="tw-text-green-600 tw-text-xs">✓ Quantity matched!</div>
            </div>
            <div class="tw-flex tw-gap-3">
              <button
                type="button"
                (click)="onClose()"
                class="nwfth-button-secondary tw-px-4 tw-py-2 tw-text-sm">
                Cancel
              </button>
              <button
                type="button"
                (click)="onConfirm()"
                [disabled]="!canConfirm()"
                class="nwfth-button-primary tw-px-4 tw-py-2 tw-text-sm tw-disabled:opacity-50 tw-disabled:cursor-not-allowed">
                Confirm Selection
              </button>
            </div>
          </div>
        </div>

      </div>
    </div>
  `
})
export class TransactionSelectionModalComponent {
  @Input() isOpen = signal(false);
  lotNo = input<string>('');
  binNo = input<string>('');
  targetQty = input<number>(0);
  @Output() transactionsSelected = new EventEmitter<number>();
  @Output() modalClosed = new EventEmitter<void>();

  private putawayService = inject(PutawayService);

  // State signals
  transactions = signal<LotTransactionItem[]>([]);
  selectedIds = signal<Set<number>>(new Set());
  isLoading = signal(false);
  errorMessage = signal<string>('');

  // Computed signals
  selectedCount = computed(() => this.selectedIds().size);

  // Sum of selected transaction quantities
  selectedTotalQty = computed(() => {
    const selected = this.selectedIds();
    const txs = this.transactions();
    return txs
      .filter(t => selected.has(t.lot_tran_no))
      .reduce((sum, t) => sum + t.qty, 0);
  });

  // Check if selected qty matches target (within small tolerance)
  isQtyMatched = computed(() => {
    const target = this.targetQty();
    if (target <= 0) return this.selectedCount() > 0; // Fallback if no target
    return Math.abs(this.selectedTotalQty() - target) < 0.001;
  });

  // Show warning when there's a mismatch (selections made but not matching)
  showQtyMismatchWarning = computed(() => {
    return this.selectedCount() > 0 && !this.isQtyMatched() && this.targetQty() > 0;
  });

  // Can only confirm when qty is matched
  canConfirm = computed(() => {
    return this.selectedCount() > 0 && this.isQtyMatched();
  });

  areAllSelected = computed(() => {
    const txs = this.transactions();
    return txs.length > 0 && txs.every(t => this.selectedIds().has(t.lot_tran_no));
  });

  constructor() {
    // Load transactions when modal opens or lotNo/binNo changes
    effect(() => {
      if (this.isOpen() && this.lotNo() && this.binNo()) {
        this.loadTransactions();
      } else if (!this.isOpen()) {
        // Reset state when closed
        this.selectedIds.set(new Set());
        this.errorMessage.set('');
      }
    });
  }

  private async loadTransactions() {
    this.isLoading.set(true);
    this.errorMessage.set('');
    this.transactions.set([]); // Clear previous
    this.selectedIds.set(new Set()); // Reset selection

    try {
      const results = await this.putawayService.searchLotTransactions(this.lotNo(), this.binNo()).toPromise();
      if (results) {
        this.transactions.set(results);
      }
    } catch (error) {
      console.error('Error loading transactions:', error);
      this.errorMessage.set('Failed to load transactions. Please try again.');
    } finally {
      this.isLoading.set(false);
    }
  }

  toggleSelection(id: number) {
    this.selectedIds.update(ids => {
      const newIds = new Set(ids);
      if (newIds.has(id)) {
        newIds.delete(id);
      } else {
        newIds.add(id);
      }
      return newIds;
    });
  }

  toggleAll(event: Event) {
    const isChecked = (event.target as HTMLInputElement).checked;
    if (isChecked) {
      const allIds = new Set(this.transactions().map(t => t.lot_tran_no));
      this.selectedIds.set(allIds);
    } else {
      this.selectedIds.set(new Set());
    }
  }

  isSelected(id: number): boolean {
    return this.selectedIds().has(id);
  }

  onConfirm() {
    // Emit the total selected quantity (already validated to match target)
    this.transactionsSelected.emit(this.selectedTotalQty());
    this.onClose();
  }

  onClose() {
    this.modalClosed.emit();
  }

  onOverlayClick(event: MouseEvent) {
    this.onClose();
  }

  trackByTranNo(index: number, item: LotTransactionItem): number {
    return item.lot_tran_no;
  }
}
