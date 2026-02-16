import { Component, Input, Output, EventEmitter, OnInit, OnDestroy } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { Subject, debounceTime, distinctUntilChanged, switchMap, catchError } from 'rxjs';
import { of } from 'rxjs';
import { cn } from '../utils';
import { 
  CommandComponent, 
  CommandInputComponent, 
  CommandListComponent, 
  CommandEmptyComponent, 
  CommandGroupComponent, 
  CommandItemComponent 
} from './command.component';

export interface SearchResult {
  value: string;
  label: string;
  description?: string;
  metadata?: Record<string, unknown>;
}

@Component({
  selector: 'app-search-input',
  standalone: true,
  imports: [
    CommonModule, 
    FormsModule,
    CommandComponent,
    CommandInputComponent,
    CommandListComponent,
    CommandEmptyComponent,
    CommandGroupComponent,
    CommandItemComponent
  ],
  template: `
    <div class="relative w-full">
      <!-- Regular input display -->
      <div *ngIf="!showDropdown" class="relative">
        <input
          [(ngModel)]="displayValue"
          (focus)="onFocus()"
          (blur)="onBlur()"
          [placeholder]="placeholder"
          [disabled]="disabled"
          [class]="cn('flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50', 'pr-10', className)"
        />
        <button 
          (click)="openSearch()"
          [disabled]="disabled"
          class="absolute right-2 top-1/2 transform -translate-y-1/2 text-muted-foreground hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50">
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"/>
          </svg>
        </button>
      </div>

      <!-- Search dropdown -->
      <div *ngIf="showDropdown" class="absolute top-0 left-0 right-0 z-50 bg-background border border-border rounded-md shadow-lg">
        <app-command className="[&_[cmdk-group-heading]]:text-muted-foreground [&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group]]:px-2 [&_[cmdk-group]:not([hidden])_~[cmdk-group]]:pt-0">
          <app-command-input
            [placeholder]="searchPlaceholder"
            [(ngModel)]="searchQuery"
            (inputChange)="onSearchChange($event)"
            (keydown)="onKeyDown($event)">
          </app-command-input>
          
          <app-command-list>
            <app-command-empty *ngIf="!loading && results.length === 0">
              {{ emptyMessage }}
            </app-command-empty>
            
            <!-- Loading state -->
            <div *ngIf="loading" class="flex items-center justify-center py-6">
              <div class="animate-spin rounded-full h-4 w-4 border-b-2 border-primary"></div>
              <span class="ml-2 text-sm text-muted-foreground">Searching...</span>
            </div>
            
            <!-- Search results -->
            <app-command-group *ngIf="!loading && results.length > 0" [heading]="groupHeading">
              <app-command-item
                *ngFor="let result of results; trackBy: trackByValue"
                [value]="result"
                (select)="selectResult($event)">
                <div class="flex flex-col">
                  <span class="font-medium">{{ result.label }}</span>
                  <span *ngIf="result.description" class="text-xs text-muted-foreground">
                    {{ result.description }}
                  </span>
                </div>
              </app-command-item>
            </app-command-group>
          </app-command-list>
        </app-command>
        
        <!-- Close button -->
        <div class="border-t border-border p-2">
          <button 
            (click)="closeSearch()"
            class="w-full text-xs text-muted-foreground hover:text-foreground py-1">
            Press Escape to close
          </button>
        </div>
      </div>
    </div>

    <!-- Overlay to capture clicks outside -->
    <div 
      *ngIf="showDropdown"
      class="fixed inset-0 z-40"
      (click)="closeSearch()">
    </div>
  `
})
export class SearchInputComponent implements OnInit, OnDestroy {
  @Input() placeholder: string = 'Select...';
  @Input() searchPlaceholder: string = 'Search...';
  @Input() className: string = '';
  @Input() disabled: boolean = false;
  @Input() emptyMessage: string = 'No results found.';
  @Input() groupHeading: string = 'Results';
  @Input() searchFunction!: (query: string) => Promise<SearchResult[]>;
  @Input() value: SearchResult | null = null;
  
  @Output() valueChange = new EventEmitter<SearchResult | null>();
  @Output() select = new EventEmitter<SearchResult>();

  displayValue: string = '';
  searchQuery: string = '';
  showDropdown: boolean = false;
  loading: boolean = false;
  results: SearchResult[] = [];
  
  private searchSubject = new Subject<string>();
  private destroy$ = new Subject<void>();

  cn = cn;

  ngOnInit() {
    // Set up debounced search
    this.searchSubject.pipe(
      debounceTime(300),
      distinctUntilChanged(),
      switchMap(query => this.performSearch(query)),
      catchError(error => {
        console.error('Search error:', error);
        return of([]);
      })
    ).subscribe(results => {
      this.results = results;
      this.loading = false;
    });

    // Set initial display value
    if (this.value) {
      this.displayValue = this.value.label;
    }
  }

  ngOnDestroy() {
    this.destroy$.next();
    this.destroy$.complete();
  }

  onFocus() {
    // Could add logic here if needed
  }

  onBlur() {
    // Delay to allow click on dropdown
    setTimeout(() => {
      if (!this.showDropdown) {
        this.closeSearch();
      }
    }, 150);
  }

  openSearch() {
    if (this.disabled) return;
    
    this.showDropdown = true;
    this.searchQuery = '';
    this.results = [];
    
    // Trigger initial search with empty query to show recent/popular items
    this.onSearchChange('');
  }

  closeSearch() {
    this.showDropdown = false;
    this.searchQuery = '';
    this.loading = false;
  }

  onSearchChange(query: string) {
    this.searchQuery = query;
    this.loading = true;
    this.searchSubject.next(query);
  }

  onKeyDown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      this.closeSearch();
    }
  }

  selectResult(result: SearchResult) {
    this.value = result;
    this.displayValue = result.label;
    this.valueChange.emit(result);
    this.select.emit(result);
    this.closeSearch();
  }

  trackByValue(index: number, item: SearchResult): string {
    return item.value;
  }

  private async performSearch(query: string): Promise<SearchResult[]> {
    if (!this.searchFunction) {
      return [];
    }

    try {
      return await this.searchFunction(query);
    } catch (error) {
      console.error('Search function error:', error);
      return [];
    }
  }
}